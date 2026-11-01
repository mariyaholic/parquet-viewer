// SPDX-License-Identifier: Apache-2.0
//! Private, bounded temporary spool for a caller-supplied stream.
//!
//! The preview handler never passes a path to the worker. It streams the
//! `IStream` contents over the protocol; the worker writes them to a new file
//! in the per-user temporary directory, reads it through the normal core, and
//! removes it when the document closes.

use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use parquet_preview_core::{PreviewError, Result};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Name prefix and suffix of the private spool files. Shared with the startup
/// sweep so a leaked spool can be recognised without touching anything else.
pub const SPOOL_PREFIX: &str = "parquet-preview-";
pub const SPOOL_SUFFIX: &str = ".tmp";

pub struct SpoolFile {
    path: PathBuf,
    file: Option<File>,
    size: u64,
    limit: u64,
}

impl SpoolFile {
    pub fn new(limit: u64) -> Result<Self> {
        if limit == 0 {
            return Err(PreviewError::Limit(
                "spool limit must be positive".to_string(),
            ));
        }
        let dir = std::env::temp_dir();
        for _ in 0..16 {
            let nonce = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = dir.join(format!(
                "{SPOOL_PREFIX}{}-{}-{}{SPOOL_SUFFIX}",
                std::process::id(),
                nonce,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.subsec_nanos())
                    .unwrap_or(0)
            ));
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                // TerminateProcess runs no destructors, so the `Drop` cleanup
                // never fires on the routine kill path (selection change, job
                // memory kill, unload). FILE_FLAG_DELETE_ON_CLOSE makes the
                // kernel delete the file when the terminated process's last
                // handle closes. DELETE access must be requested explicitly.
                const GENERIC_READ: u32 = 0x8000_0000;
                const GENERIC_WRITE: u32 = 0x4000_0000;
                const DELETE: u32 = 0x0001_0000;
                const FILE_SHARE_READ: u32 = 0x0000_0001;
                const FILE_SHARE_DELETE: u32 = 0x0000_0004;
                const FILE_FLAG_DELETE_ON_CLOSE: u32 = 0x0400_0000;
                options.access_mode(GENERIC_READ | GENERIC_WRITE | DELETE);
                options.share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE);
                options.custom_flags(FILE_FLAG_DELETE_ON_CLOSE);
            }
            match options.open(&path) {
                Ok(file) => {
                    return Ok(Self {
                        path,
                        file: Some(file),
                        size: 0,
                        limit,
                    })
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(PreviewError::Limit(
            "could not create a private spool file".to_string(),
        ))
    }

    pub fn append(&mut self, chunk: &[u8]) -> Result<()> {
        let next = self.size.saturating_add(chunk.len() as u64);
        if next > self.limit {
            return Err(PreviewError::FileTooLarge {
                size: next,
                limit: self.limit,
            });
        }
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| PreviewError::Stream("spool is already finalized".to_string()))?;
        file.write_all(chunk)?;
        self.size = next;
        Ok(())
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn limit(&self) -> u64 {
        self.limit
    }

    /// Flush and hand the file to the reader. The path stays owned here for
    /// cleanup when the spool is dropped.
    pub fn take_file(&mut self) -> Result<File> {
        let file = self
            .file
            .take()
            .ok_or_else(|| PreviewError::Stream("spool is already finalized".to_string()))?;
        let mut file = file;
        file.flush()?;
        file.seek(SeekFrom::Start(0))?;
        Ok(file)
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for SpoolFile {
    fn drop(&mut self) {
        self.file.take();
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Delete leftover `parquet-preview-*.tmp` files older than `max_age`.
///
/// Delete-on-close (Windows) and the `Drop` cleanup cover the normal cases;
/// this sweeps files leaked by a crashed process or an older build. Best
/// effort: only files matching the spool prefix/suffix are considered, a live
/// worker's file is younger than `max_age` under normal use, and every error
/// is ignored. Returns how many files were removed (for tests).
pub fn sweep_stale_spools(max_age: std::time::Duration) -> usize {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return 0;
    };
    let now = std::time::SystemTime::now();
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(SPOOL_PREFIX) || !name.ends_with(SPOOL_SUFFIX) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if now.duration_since(modified).unwrap_or_default() < max_age {
            continue;
        }
        if std::fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn append_and_read_back() {
        let mut spool = SpoolFile::new(1024).unwrap();
        spool.append(b"hello ").unwrap();
        spool.append(b"world").unwrap();
        let path = spool.path().to_path_buf();
        assert_eq!(spool.size(), 11);
        let mut file = spool.take_file().unwrap();
        let mut text = String::new();
        file.read_to_string(&mut text).unwrap();
        assert_eq!(text, "hello world");
        drop(file); // Windows cannot remove the spool while this handle is open.
        drop(spool);
        assert!(!path.exists());
    }

    #[test]
    fn limit_is_enforced() {
        let mut spool = SpoolFile::new(4).unwrap();
        spool.append(b"abcd").unwrap();
        assert!(matches!(
            spool.append(b"e"),
            Err(PreviewError::FileTooLarge { .. })
        ));
    }

    #[test]
    fn zero_limit_is_rejected() {
        assert!(SpoolFile::new(0).is_err());
    }

    #[test]
    fn spools_are_unique_and_private() {
        let a = SpoolFile::new(10).unwrap();
        let b = SpoolFile::new(10).unwrap();
        assert_ne!(a.path(), b.path());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(a.path()).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn sweep_removes_only_old_spool_files() {
        let dir = std::env::temp_dir();
        let uniq = format!(
            "{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let old = dir.join(format!("{SPOOL_PREFIX}sweep-old-{uniq}{SPOOL_SUFFIX}"));
        let fresh = dir.join(format!("{SPOOL_PREFIX}sweep-new-{uniq}{SPOOL_SUFFIX}"));
        let unrelated = dir.join(format!("not-a-spool-{uniq}.tmp"));
        std::fs::write(&old, b"old").unwrap();
        std::fs::write(&fresh, b"new").unwrap();
        std::fs::write(&unrelated, b"x").unwrap();
        // Age the first file past the sweep threshold.
        let two_hours = std::time::Duration::from_secs(2 * 3600);
        let when = std::time::SystemTime::now() - two_hours;
        std::fs::File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(when)
            .unwrap();

        let removed = sweep_stale_spools(std::time::Duration::from_secs(3600));
        assert!(removed >= 1, "the aged spool should have been swept");
        assert!(!old.exists(), "an old spool file must be removed");
        assert!(fresh.exists(), "a fresh spool file must be kept");
        assert!(unrelated.exists(), "a non-spool temp file must be kept");

        let _ = std::fs::remove_file(&fresh);
        let _ = std::fs::remove_file(&unrelated);
    }
}

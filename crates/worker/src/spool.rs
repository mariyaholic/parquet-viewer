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

/// Spool naming convention, not proof of ownership. Never use these names or
/// file ages to decide whether another process's file can be deleted.
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
            #[cfg(target_os = "linux")]
            {
                use std::os::unix::fs::OpenOptionsExt;
                // O_TMPFILE creates an unnamed inode: no pathname cleanup can
                // race with replacement. Refuse unsupported filesystems rather
                // than fall back to deleting a shared-temp name.
                const O_TMPFILE: i32 = 0x410000;
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .custom_flags(O_TMPFILE)
                    .mode(0o600)
                    .open(&dir)?;
                return Ok(Self {
                    path,
                    file: Some(file),
                    size: 0,
                    limit,
                });
            }
            #[cfg(not(any(target_os = "linux", windows)))]
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "handle-owned spools are supported on Linux and Windows",
            )
            .into());
            #[cfg(windows)]
            {
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

    /// Flush and hand the owned file to the reader. The last handle cleans up
    /// the unnamed Linux inode or Windows delete-on-close object.
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

    /// Diagnostic name. Linux spools are anonymous and have no directory entry.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for SpoolFile {
    fn drop(&mut self) {
        self.file.take();
        // Never delete by pathname: it may now name someone else's file.
    }
}

/// Legacy compatibility hook. Automatic shared-temp cleanup is disabled:
/// matching names and ages do not prove ownership or that a worker is dead.
/// Only the creating SpoolFile/Windows delete-on-close handle performs cleanup.
pub fn sweep_stale_spools(_max_age: std::time::Duration) -> usize {
    0
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
            let mode = a
                .file
                .as_ref()
                .unwrap()
                .metadata()
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn legacy_sweep_never_removes_unrelated_or_active_files() {
        let mut active = SpoolFile::new(10).unwrap();
        active.append(b"active").unwrap();
        active
            .file
            .as_ref()
            .unwrap()
            .set_modified(std::time::UNIX_EPOCH)
            .unwrap();
        // Create only our own synthetic fixture, with exclusive creation.
        let path = active.path().with_file_name(format!(
            "{SPOOL_PREFIX}unrelated-{}-{}{SPOOL_SUFFIX}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let mut unrelated = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        unrelated.write_all(b"unrelated fixture").unwrap();
        unrelated.set_modified(std::time::UNIX_EPOCH).unwrap();
        drop(unrelated);
        assert_eq!(sweep_stale_spools(std::time::Duration::ZERO), 0);
        assert_eq!(std::fs::read(&path).unwrap(), b"unrelated fixture");
        assert_eq!(active.file.as_ref().unwrap().metadata().unwrap().len(), 6);
        std::fs::remove_file(path).unwrap(); // only the fixture this test created
    }

    #[test]
    fn dropping_spool_keeps_a_replacement_at_its_old_name() {
        let mut spool = SpoolFile::new(16).unwrap();
        spool.append(b"owned").unwrap();
        let path = spool.path().to_path_buf();
        drop(spool.take_file().unwrap());
        // The owned handle is now closed. Only this synthetic fixture is
        // created at the diagnostic name; cleanup must not delete it.
        let mut replacement = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        replacement.write_all(b"replacement").unwrap();
        drop(replacement);
        drop(spool);
        assert_eq!(std::fs::read(&path).unwrap(), b"replacement");
        std::fs::remove_file(path).unwrap();
    }
}

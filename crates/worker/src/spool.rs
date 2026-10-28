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
                "parquet-preview-{}-{}-{}.tmp",
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
}

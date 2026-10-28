// SPDX-License-Identifier: Apache-2.0
//! Bounded, seekable access to a caller-supplied stream.
//!
//! The Windows preview handler receives an `IStream`, never a path. This module
//! wraps that stream and implements the Parquet crate's `ChunkReader` so the
//! reader can seek within the caller's data without assuming a filesystem
//! location.

use std::io::{self, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use parquet::file::reader::ChunkReader;

/// Object-safe combination of `Read`, `Seek` and `Send`.
pub trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send + ?Sized> ReadSeek for T {}

/// A caller-owned stream, moved into the inspector.
pub type BoxSource = Box<dyn ReadSeek>;

/// Cloneable handle over a single seekable stream. Reads are serialized by a
/// mutex; each child reader seeks to its own logical position before reading so
/// interleaved readers cannot corrupt one another.
#[derive(Clone)]
pub struct SharedSource {
    inner: Arc<Mutex<BoxSource>>,
    len: u64,
}

impl SharedSource {
    pub fn new(mut source: BoxSource) -> crate::Result<Self> {
        let len = source.seek(SeekFrom::End(0))?;
        source.seek(SeekFrom::Start(0))?;
        Ok(Self {
            inner: Arc::new(Mutex::new(source)),
            len,
        })
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn lock(&self) -> io::Result<std::sync::MutexGuard<'_, BoxSource>> {
        self.inner
            .lock()
            .map_err(|_| io::Error::other("stream lock poisoned"))
    }

    pub fn read_at(&self, start: u64, length: usize) -> io::Result<Vec<u8>> {
        let mut guard = self.lock()?;
        guard.seek(SeekFrom::Start(start))?;
        let mut buffer = vec![0u8; length];
        guard.read_exact(&mut buffer)?;
        Ok(buffer)
    }
}

impl Read for SharedSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.lock()?.read(buf)
    }
}

impl Seek for SharedSource {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.lock()?.seek(pos)
    }
}

/// Independent logical cursor over [`SharedSource`].
pub struct SharedSourceReader {
    source: SharedSource,
    pos: u64,
}

impl Read for SharedSourceReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut guard = self.source.lock()?;
        guard.seek(SeekFrom::Start(self.pos))?;
        let read = guard.read(buf)?;
        self.pos = self.pos.saturating_add(read as u64);
        Ok(read)
    }
}

impl Seek for SharedSourceReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let next = match pos {
            SeekFrom::Start(n) => n,
            SeekFrom::Current(delta) => self.pos.saturating_add_signed(delta),
            SeekFrom::End(delta) => self.source.len.saturating_add_signed(delta),
        };
        self.pos = next;
        Ok(self.pos)
    }
}

impl parquet::file::reader::Length for SharedSource {
    fn len(&self) -> u64 {
        self.len
    }
}

impl ChunkReader for SharedSource {
    type T = SharedSourceReader;

    fn get_read(&self, start: u64) -> parquet::errors::Result<Self::T> {
        Ok(SharedSourceReader {
            source: self.clone(),
            pos: start,
        })
    }

    fn get_bytes(&self, start: u64, length: usize) -> parquet::errors::Result<Bytes> {
        let data = self
            .read_at(start, length)
            .map_err(|e| parquet::errors::ParquetError::EOF(e.to_string()))?;
        Ok(Bytes::from(data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn source(data: &'static [u8]) -> SharedSource {
        SharedSource::new(Box::new(Cursor::new(data))).unwrap()
    }

    #[test]
    fn reports_length() {
        let s = source(b"abcdef");
        assert_eq!(s.len(), 6);
        assert!(!s.is_empty());
    }

    #[test]
    fn readers_do_not_share_cursor() {
        let s = source(b"0123456789");
        let mut a = s.get_read(2).unwrap();
        let mut b = s.get_read(7).unwrap();
        let mut one = [0u8; 2];
        let mut two = [0u8; 2];
        a.read_exact(&mut one).unwrap();
        b.read_exact(&mut two).unwrap();
        assert_eq!(&one, b"23");
        assert_eq!(&two, b"78");
        let mut again = [0u8; 1];
        a.read_exact(&mut again).unwrap();
        assert_eq!(&again, b"4");
    }

    #[test]
    fn get_bytes_is_exact() {
        let s = source(b"0123456789");
        assert_eq!(&s.get_bytes(3, 4).unwrap()[..], b"3456");
        assert!(s.get_bytes(8, 8).is_err());
    }

    #[test]
    fn seek_end_and_current() {
        let s = source(b"0123456789");
        let mut r = s.get_read(0).unwrap();
        r.seek(SeekFrom::End(-2)).unwrap();
        let mut buf = [0u8; 2];
        r.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"89");
        r.seek(SeekFrom::Current(-1)).unwrap();
        r.read_exact(&mut buf[..1]).unwrap();
        assert_eq!(&buf[..1], b"9");
    }
}

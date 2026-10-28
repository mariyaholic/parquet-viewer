// SPDX-License-Identifier: Apache-2.0
//! Bounded, exact stream snapshot before handing data to another thread.

use std::io::{self, Read};

pub const MAX_SNAPSHOT_BYTES: u64 = 64 * 1024 * 1024;

pub fn snapshot(reader: &mut impl Read, size: u64, limit: u64) -> io::Result<Vec<u8>> {
    if size == 0 || size > limit || size > usize::MAX as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("stream size {size} exceeds snapshot limit {limit} or is empty"),
        ));
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size as usize)
        .map_err(|_| io::Error::other("cannot allocate stream snapshot"))?;
    let mut bounded = reader.take(size);
    bounded.read_to_end(&mut bytes)?;
    if bytes.len() as u64 != size {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "stream ended before its declared size",
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copies_exactly_declared_bytes() {
        assert_eq!(snapshot(&mut &b"abcdef"[..], 3, 3).unwrap(), b"abc");
    }
    #[test]
    fn rejects_oversize_empty_and_truncated() {
        assert!(snapshot(&mut &b"abc"[..], MAX_SNAPSHOT_BYTES + 1, MAX_SNAPSHOT_BYTES).is_err());
        assert!(snapshot(&mut &b"abc"[..], 4, 3).is_err());
        assert!(snapshot(&mut &b""[..], 0, 3).is_err());
        assert_eq!(
            snapshot(&mut &b"ab"[..], 3, 3).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }
}

// SPDX-License-Identifier: Apache-2.0
//! Minimal, defensive Parquet footer parsing.
//!
//! The full readers re-read this structure; parsing it first lets the preview
//! reject non-Parquet input, encrypted files and oversized footers before
//! handing bytes to a decoder.

use std::io::{Read, Seek, SeekFrom};

use serde::{Deserialize, Serialize};

use crate::{PreviewError, Result};

pub const MAGIC: [u8; 4] = *b"PAR1";
pub const ENCRYPTED_MAGIC: [u8; 4] = *b"PARE";
const MIN_FILE_BYTES: u64 = 12;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FooterInfo {
    pub file_bytes: u64,
    pub footer_bytes: u32,
}

/// Read and validate the header magic and the eight-byte trailer.
pub fn parse_footer<R: Read + Seek>(
    reader: &mut R,
    file_bytes: u64,
    max_footer_bytes: u64,
) -> Result<FooterInfo> {
    if file_bytes < MIN_FILE_BYTES {
        return Err(PreviewError::NotParquet(format!(
            "file is only {file_bytes} bytes; a Parquet file needs at least {MIN_FILE_BYTES}"
        )));
    }

    reader.seek(SeekFrom::Start(0))?;
    let mut header = [0u8; 4];
    reader.read_exact(&mut header)?;
    if header == ENCRYPTED_MAGIC {
        return Err(PreviewError::Encrypted);
    }
    if header != MAGIC {
        return Err(PreviewError::NotParquet(
            "missing PAR1 header magic".to_string(),
        ));
    }

    reader.seek(SeekFrom::End(-8))?;
    let mut trailer = [0u8; 8];
    reader.read_exact(&mut trailer)?;
    let footer_bytes = u32::from_le_bytes([trailer[0], trailer[1], trailer[2], trailer[3]]);
    let tail_magic = [trailer[4], trailer[5], trailer[6], trailer[7]];
    if tail_magic == ENCRYPTED_MAGIC {
        return Err(PreviewError::Encrypted);
    }
    if tail_magic != MAGIC {
        return Err(PreviewError::NotParquet(
            "missing PAR1 footer magic".to_string(),
        ));
    }

    let footer = u64::from(footer_bytes);
    if footer == 0 {
        return Err(PreviewError::MalformedFooter(
            "footer length is zero".to_string(),
        ));
    }
    if footer > max_footer_bytes {
        return Err(PreviewError::FooterTooLarge {
            size: footer,
            limit: max_footer_bytes,
        });
    }
    if footer + MIN_FILE_BYTES > file_bytes {
        return Err(PreviewError::MalformedFooter(format!(
            "footer length {footer} exceeds the {file_bytes}-byte file"
        )));
    }

    Ok(FooterInfo {
        file_bytes,
        footer_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn trailer(footer: u32, magic: &[u8; 4]) -> Vec<u8> {
        let mut v = footer.to_le_bytes().to_vec();
        v.extend_from_slice(magic);
        v
    }

    fn file(body: &[u8], footer: u32, magic: &[u8; 4]) -> Vec<u8> {
        let mut v = b"PAR1".to_vec();
        v.extend_from_slice(body);
        v.extend_from_slice(&trailer(footer, magic));
        v
    }

    #[test]
    fn accepts_minimal_footer() {
        let data = file(b"x", 1, b"PAR1");
        let mut c = Cursor::new(&data);
        let info = parse_footer(&mut c, data.len() as u64, 1024).unwrap();
        assert_eq!(info.footer_bytes, 1);
    }

    #[test]
    fn rejects_short_file() {
        let mut c = Cursor::new(b"PAR1PAR1");
        let err = parse_footer(&mut c, 8, 1024).unwrap_err();
        assert!(matches!(err, PreviewError::NotParquet(_)));
    }

    #[test]
    fn rejects_wrong_header() {
        let mut data = file(b"x", 1, b"PAR1");
        data[0] = b'X';
        let mut c = Cursor::new(&data);
        let err = parse_footer(&mut c, data.len() as u64, 1024).unwrap_err();
        assert!(matches!(err, PreviewError::NotParquet(_)));
    }

    #[test]
    fn rejects_wrong_footer_magic() {
        let data = file(b"x", 1, b"XXXX");
        let mut c = Cursor::new(&data);
        let err = parse_footer(&mut c, data.len() as u64, 1024).unwrap_err();
        assert!(matches!(err, PreviewError::NotParquet(_)));
    }

    #[test]
    fn detects_encrypted_magic() {
        let data = file(b"x", 1, b"PARE");
        let mut c = Cursor::new(&data);
        assert!(matches!(
            parse_footer(&mut c, data.len() as u64, 1024),
            Err(PreviewError::Encrypted)
        ));

        let mut header_encrypted = file(b"x", 1, b"PAR1");
        header_encrypted[0..4].copy_from_slice(b"PARE");
        let mut c = Cursor::new(&header_encrypted);
        assert!(matches!(
            parse_footer(&mut c, header_encrypted.len() as u64, 1024),
            Err(PreviewError::Encrypted)
        ));
    }

    #[test]
    fn rejects_zero_footer() {
        let data = file(b"x", 0, b"PAR1");
        let mut c = Cursor::new(&data);
        assert!(matches!(
            parse_footer(&mut c, data.len() as u64, 1024),
            Err(PreviewError::MalformedFooter(_))
        ));
    }

    #[test]
    fn rejects_footer_larger_than_file() {
        let data = file(b"x", 999, b"PAR1");
        let mut c = Cursor::new(&data);
        assert!(matches!(
            parse_footer(&mut c, data.len() as u64, 1024),
            Err(PreviewError::MalformedFooter(_))
        ));
    }

    #[test]
    fn enforces_footer_budget() {
        let data = file(&vec![0u8; 64], 64, b"PAR1");
        let mut c = Cursor::new(&data);
        assert!(matches!(
            parse_footer(&mut c, data.len() as u64, 32),
            Err(PreviewError::FooterTooLarge { .. })
        ));
    }
}

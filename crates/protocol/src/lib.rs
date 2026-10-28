// SPDX-License-Identifier: Apache-2.0
//! Versioned, bounded worker protocol.
//!
//! Transport is a sequence of frames over stdin/stdout of an isolated worker
//! process. Each frame is a 4-byte big-endian payload length, one tag byte and
//! the payload. Control frames carry UTF-8 JSON; stream frames carry raw bytes
//! that the worker spools privately. Both frame classes have hard size caps and
//! the version is checked before any file is accepted.

use std::io::{Read, Write};

use parquet_preview_core::baseline::BaselineSpec;
use parquet_preview_core::market::MarketReport;
use parquet_preview_core::sample::SampleReport;
use parquet_preview_core::scan::ScanReport;
use parquet_preview_core::summary::Summary;
use parquet_preview_core::{PreviewOptions, PreviewReport};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;
pub const TAG_CONTROL: u8 = 1;
pub const TAG_STREAM: u8 = 2;
pub const DEFAULT_MAX_CONTROL_FRAME: usize = 8 * 1024 * 1024;
pub const DEFAULT_MAX_STREAM_CHUNK: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameLimits {
    pub max_control: usize,
    pub max_stream_chunk: usize,
}

impl Default for FrameLimits {
    fn default() -> Self {
        Self {
            max_control: DEFAULT_MAX_CONTROL_FRAME,
            max_stream_chunk: DEFAULT_MAX_STREAM_CHUNK,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    Control(Vec<u8>),
    StreamChunk(Vec<u8>),
}

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("truncated frame header")]
    TruncatedHeader,
    #[error("frame of {length} bytes exceeds the {max}-byte cap for tag {tag}")]
    FrameTooLarge {
        tag: u8,
        length: usize,
        max: usize,
    },
    #[error("unknown frame tag {0}")]
    UnknownTag(u8),
    #[error("invalid control JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("control payload is not valid UTF-8")]
    Utf8,
}

/// Read one frame, or `None` at a clean end of stream.
pub fn read_frame<R: Read>(
    reader: &mut R,
    limits: &FrameLimits,
) -> Result<Option<Frame>, ProtocolError> {
    let mut header = [0u8; 5];
    let mut filled = 0usize;
    while filled < header.len() {
        match reader.read(&mut header[filled..]) {
            Ok(0) => {
                if filled == 0 {
                    return Ok(None);
                }
                return Err(ProtocolError::TruncatedHeader);
            }
            Ok(n) => filled += n,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(ProtocolError::Io(error)),
        }
    }
    let length = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
    let tag = header[4];
    let max = match tag {
        TAG_CONTROL => limits.max_control,
        TAG_STREAM => limits.max_stream_chunk,
        other => return Err(ProtocolError::UnknownTag(other)),
    };
    if length > max {
        return Err(ProtocolError::FrameTooLarge { tag, length, max });
    }
    let mut payload = vec![0u8; length];
    reader.read_exact(&mut payload)?;
    match tag {
        TAG_CONTROL => Ok(Some(Frame::Control(payload))),
        TAG_STREAM => Ok(Some(Frame::StreamChunk(payload))),
        _ => unreachable!("tag validated above"),
    }
}

pub fn write_control<W: Write, T: Serialize>(
    writer: &mut W,
    message: &T,
) -> Result<(), ProtocolError> {
    let payload = serde_json::to_vec(message)?;
    write_frame(writer, TAG_CONTROL, &payload, &FrameLimits::default())?;
    writer.flush()?;
    Ok(())
}

pub fn write_stream_chunk<W: Write>(
    writer: &mut W,
    chunk: &[u8],
) -> Result<(), ProtocolError> {
    if chunk.len() > DEFAULT_MAX_STREAM_CHUNK {
        return Err(ProtocolError::FrameTooLarge {
            tag: TAG_STREAM,
            length: chunk.len(),
            max: DEFAULT_MAX_STREAM_CHUNK,
        });
    }
    write_frame(writer, TAG_STREAM, chunk, &FrameLimits::default())?;
    writer.flush()?;
    Ok(())
}

fn write_frame<W: Write>(
    writer: &mut W,
    tag: u8,
    payload: &[u8],
    limits: &FrameLimits,
) -> Result<(), ProtocolError> {
    let max = match tag {
        TAG_CONTROL => limits.max_control,
        TAG_STREAM => limits.max_stream_chunk,
        _ => {
            return Err(ProtocolError::UnknownTag(tag));
        }
    };
    if payload.len() > max || payload.len() > u32::MAX as usize {
        return Err(ProtocolError::FrameTooLarge {
            tag,
            length: payload.len(),
            max,
        });
    }
    writer.write_all(&(payload.len() as u32).to_be_bytes())?;
    writer.write_all(&[tag])?;
    writer.write_all(payload)?;
    Ok(())
}

pub fn decode_control<T: DeserializeOwned>(frame: Frame) -> Result<T, ProtocolError> {
    match frame {
        Frame::Control(payload) => Ok(serde_json::from_slice(&payload)?),
        Frame::StreamChunk(_) => Err(ProtocolError::UnknownTag(TAG_STREAM)),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Hello {
        protocol_version: u32,
        #[serde(default)]
        client: Option<String>,
    },
    OpenStream {
        declared_size: u64,
    },
    StreamAbort,
    Inspect,
    Sample {
        offset: u64,
        limit: u32,
    },
    Scan,
    Diagnose {
        market_profile: Option<parquet_preview_core::market::MarketProfile>,
        baseline: Option<BaselineSpec>,
        #[serde(default)]
        rows: Option<u32>,
    },
    Render {
        options: PreviewOptions,
    },
    About,
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    ProtocolMismatch,
    ProtocolError,
    StreamTooLarge,
    StreamIncomplete,
    NoFile,
    FileError,
    ProfileMissing,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Welcome {
        worker_version: String,
        protocol_version: u32,
        max_file_bytes: u64,
        max_control_frame: u64,
        max_stream_chunk: u64,
    },
    StreamAccepted {
        bytes: u64,
    },
    StreamAborted,
    Summary {
        summary: Summary,
    },
    Sample {
        sample: SampleReport,
    },
    Scan {
        scan: ScanReport,
    },
    Market {
        market: MarketReport,
    },
    Preview {
        report: PreviewReport,
        text: String,
    },
    About {
        text: String,
    },
    Error {
        code: ErrorCode,
        message: String,
    },
    Bye,
}

/// Truncate an error message so a hostile input cannot flood the host.
pub fn bounded_message(message: &str) -> String {
    message.chars().take(500).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn roundtrip(frame: &Frame) -> Frame {
        let mut bytes = Vec::new();
        let limits = FrameLimits::default();
        let (tag, payload) = match frame {
            Frame::Control(p) => (TAG_CONTROL, p.as_slice()),
            Frame::StreamChunk(p) => (TAG_STREAM, p.as_slice()),
        };
        write_frame(&mut bytes, tag, payload, &limits).unwrap();
        read_frame(&mut Cursor::new(bytes), &limits)
            .unwrap()
            .unwrap()
    }

    #[test]
    fn frames_round_trip() {
        assert_eq!(
            roundtrip(&Frame::Control(b"{\"a\":1}".to_vec())),
            Frame::Control(b"{\"a\":1}".to_vec())
        );
        assert_eq!(
            roundtrip(&Frame::StreamChunk(vec![0, 1, 2, 255])),
            Frame::StreamChunk(vec![0, 1, 2, 255])
        );
    }

    #[test]
    fn clean_eof_is_none() {
        let limits = FrameLimits::default();
        assert_eq!(read_frame(&mut Cursor::new(Vec::new()), &limits).unwrap(), None);
    }

    #[test]
    fn truncated_header_is_rejected() {
        let limits = FrameLimits::default();
        assert!(matches!(
            read_frame(&mut Cursor::new(vec![0, 0, 0]), &limits),
            Err(ProtocolError::TruncatedHeader)
        ));
    }

    #[test]
    fn truncated_payload_is_rejected() {
        let limits = FrameLimits::default();
        let mut bytes = vec![0, 0, 0, 6, TAG_CONTROL];
        bytes.extend_from_slice(b"abc");
        assert!(matches!(
            read_frame(&mut Cursor::new(bytes), &limits),
            Err(ProtocolError::Io(_))
        ));
    }

    #[test]
    fn unknown_tag_is_rejected() {
        let limits = FrameLimits::default();
        let bytes = vec![0, 0, 0, 0, 99];
        assert!(matches!(
            read_frame(&mut Cursor::new(bytes), &limits),
            Err(ProtocolError::UnknownTag(99))
        ));
    }

    #[test]
    fn control_and_stream_caps_are_enforced() {
        let limits = FrameLimits {
            max_control: 4,
            max_stream_chunk: 2,
        };
        let control = vec![0, 0, 0, 5, TAG_CONTROL, b'a', b'b', b'c', b'd', b'e'];
        assert!(matches!(
            read_frame(&mut Cursor::new(control), &limits),
            Err(ProtocolError::FrameTooLarge { tag: TAG_CONTROL, .. })
        ));
        let chunk = vec![0, 0, 0, 3, TAG_STREAM, 1, 2, 3];
        assert!(matches!(
            read_frame(&mut Cursor::new(chunk), &limits),
            Err(ProtocolError::FrameTooLarge { tag: TAG_STREAM, .. })
        ));
    }

    #[test]
    fn oversized_write_is_refused() {
        let limits = FrameLimits {
            max_control: 2,
            max_stream_chunk: 2,
        };
        let mut out = Vec::new();
        assert!(write_frame(&mut out, TAG_CONTROL, b"abc", &limits).is_err());
        assert!(out.is_empty());
    }

    #[test]
    fn requests_and_responses_round_trip() {
        let requests = vec![
            Request::Hello {
                protocol_version: PROTOCOL_VERSION,
                client: Some("test".into()),
            },
            Request::OpenStream { declared_size: 42 },
            Request::Sample {
                offset: 0,
                limit: 10,
            },
            Request::Render {
                options: PreviewOptions::default(),
            },
            Request::About,
            Request::Shutdown,
        ];
        for request in requests {
            let json = serde_json::to_vec(&request).unwrap();
            let back: Request = serde_json::from_slice(&json).unwrap();
            assert_eq!(back, request);
        }
        let response = Response::Error {
            code: ErrorCode::NoFile,
            message: "no file".into(),
        };
        let json = serde_json::to_vec(&response).unwrap();
        assert_eq!(serde_json::from_slice::<Response>(&json).unwrap(), response);
    }

    #[test]
    fn decode_control_rejects_stream_frame() {
        assert!(decode_control::<Request>(Frame::StreamChunk(vec![])).is_err());
    }

    #[test]
    fn error_messages_are_bounded() {
        assert_eq!(bounded_message(&"a".repeat(1000)).chars().count(), 500);
    }
}

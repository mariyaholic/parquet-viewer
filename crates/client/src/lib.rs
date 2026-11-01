// SPDX-License-Identifier: Apache-2.0
//! Spawn and drive the isolated worker over its versioned protocol.
//!
//! Cancellation is by process termination plus transfer interruption: the host
//! keeps the child handle and kills it when a preview is replaced or the client
//! is dropped, and a stream transfer stops between chunks once cancellation is
//! requested. There is no wall-clock deadline: a single stuck decode runs until
//! the next replacement.

use std::io::{BufReader, Read};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use parquet_preview_core::baseline::BaselineSpec;
use parquet_preview_core::market::{MarketProfile, MarketReport};
use parquet_preview_core::sample::SampleReport;
use parquet_preview_core::scan::ScanReport;
use parquet_preview_core::summary::Summary;
use parquet_preview_core::{PreviewOptions, PreviewReport};
use parquet_preview_protocol::{
    decode_control, read_frame, write_control, write_stream_chunk, ErrorCode, Frame, FrameLimits,
    Request, Response, PROTOCOL_VERSION,
};

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("worker I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("worker protocol error: {0}")]
    Protocol(#[from] parquet_preview_protocol::ProtocolError),
    #[error("worker error ({code:?}): {message}")]
    Worker { code: ErrorCode, message: String },
    #[error("unexpected worker response: {0}")]
    Unexpected(String),
    #[error("the worker closed the connection")]
    Closed,
    #[error("the stream ended after {actual} of {expected} bytes")]
    ShortStream { actual: u64, expected: u64 },
}

pub type Result<T> = std::result::Result<T, ClientError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Welcome {
    pub worker_version: String,
    pub protocol_version: u32,
    pub max_file_bytes: u64,
    pub max_control_frame: u64,
    pub max_stream_chunk: u64,
}

pub struct WorkerClient {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    frames: FrameLimits,
    welcome: Welcome,
    finished: bool,
}

impl WorkerClient {
    /// Spawn `worker_path` and complete the protocol handshake.
    pub fn spawn(worker_path: &Path, client_name: &str) -> Result<Self> {
        let mut command = Command::new(worker_path);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command.spawn()?;
        let input = child.stdin.take().ok_or(ClientError::Closed)?;
        let output = child.stdout.take().ok_or(ClientError::Closed)?;
        let mut client = Self {
            child,
            input,
            output: BufReader::new(output),
            frames: FrameLimits::default(),
            welcome: Welcome {
                worker_version: String::new(),
                protocol_version: 0,
                max_file_bytes: 0,
                max_control_frame: 0,
                max_stream_chunk: 0,
            },
            finished: false,
        };
        client.send(&Request::Hello {
            protocol_version: PROTOCOL_VERSION,
            client: Some(client_name.chars().take(80).collect()),
        })?;
        match client.read()? {
            Response::Welcome {
                worker_version,
                protocol_version,
                max_file_bytes,
                max_control_frame,
                max_stream_chunk,
            } => {
                if protocol_version != PROTOCOL_VERSION {
                    return Err(ClientError::Worker {
                        code: ErrorCode::ProtocolMismatch,
                        message: format!(
                            "worker protocol {protocol_version}, client protocol {PROTOCOL_VERSION}"
                        ),
                    });
                }
                if worker_version != env!("CARGO_PKG_VERSION") {
                    return Err(ClientError::Worker {
                        code: ErrorCode::ProtocolMismatch,
                        message: format!(
                            "worker version {worker_version} does not match client version {}",
                            env!("CARGO_PKG_VERSION")
                        ),
                    });
                }
                client.welcome = Welcome {
                    worker_version,
                    protocol_version,
                    max_file_bytes,
                    max_control_frame,
                    max_stream_chunk,
                };
                Ok(client)
            }
            other => Err(ClientError::Unexpected(format!("{other:?}"))),
        }
    }

    pub fn welcome(&self) -> &Welcome {
        &self.welcome
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Stream a caller-supplied reader into the worker's private spool.
    pub fn open_stream(&mut self, reader: &mut dyn Read, declared_size: u64) -> Result<()> {
        if declared_size > self.welcome.max_file_bytes {
            return Err(ClientError::Worker {
                code: ErrorCode::StreamTooLarge,
                message: format!(
                    "{declared_size} bytes exceeds the worker's {}-byte limit",
                    self.welcome.max_file_bytes
                ),
            });
        }
        self.send(&Request::OpenStream { declared_size })?;
        let mut remaining = declared_size;
        let mut buffer = vec![0u8; self.welcome.max_stream_chunk.min(64 * 1024) as usize];
        while remaining > 0 {
            let want = (buffer.len() as u64).min(remaining) as usize;
            let read = reader.read(&mut buffer[..want])?;
            if read == 0 {
                let _ = self.send(&Request::StreamAbort);
                let _ = self.read();
                return Err(ClientError::ShortStream {
                    actual: declared_size - remaining,
                    expected: declared_size,
                });
            }
            write_stream_chunk(&mut self.input, &buffer[..read])?;
            remaining -= read as u64;
        }
        let response = self.read()?;
        match response {
            Response::StreamAccepted { bytes } if bytes == declared_size => Ok(()),
            Response::Error { code, message } => Err(ClientError::Worker { code, message }),
            other => Err(ClientError::Unexpected(format!("{other:?}"))),
        }
    }

    pub fn inspect(&mut self) -> Result<Summary> {
        self.expect_summary(Request::Inspect)
    }

    pub fn sample(&mut self, offset: u64, limit: usize) -> Result<SampleReport> {
        self.send(&Request::Sample {
            offset,
            limit: limit.min(u32::MAX as usize) as u32,
        })?;
        match self.read()? {
            Response::Sample { sample } => Ok(sample),
            Response::Error { code, message } => Err(ClientError::Worker { code, message }),
            other => Err(ClientError::Unexpected(format!("{other:?}"))),
        }
    }

    pub fn scan(&mut self) -> Result<ScanReport> {
        self.send(&Request::Scan)?;
        match self.read()? {
            Response::Scan { scan } => Ok(scan),
            Response::Error { code, message } => Err(ClientError::Worker { code, message }),
            other => Err(ClientError::Unexpected(format!("{other:?}"))),
        }
    }

    pub fn diagnose(
        &mut self,
        profile: Option<MarketProfile>,
        baseline: Option<BaselineSpec>,
        rows: Option<usize>,
    ) -> Result<MarketReport> {
        self.send(&Request::Diagnose {
            market_profile: profile,
            baseline,
            rows: rows.map(|v| v.min(u32::MAX as usize) as u32),
        })?;
        match self.read()? {
            Response::Market { market } => Ok(market),
            Response::Error { code, message } => Err(ClientError::Worker { code, message }),
            other => Err(ClientError::Unexpected(format!("{other:?}"))),
        }
    }

    pub fn render(&mut self, options: PreviewOptions) -> Result<(PreviewReport, String)> {
        self.send(&Request::Render { options })?;
        match self.read()? {
            Response::Preview { report, text } => Ok((report, text)),
            Response::Error { code, message } => Err(ClientError::Worker { code, message }),
            other => Err(ClientError::Unexpected(format!("{other:?}"))),
        }
    }

    pub fn about(&mut self) -> Result<String> {
        self.send(&Request::About)?;
        match self.read()? {
            Response::About { text } => Ok(text),
            Response::Error { code, message } => Err(ClientError::Worker { code, message }),
            other => Err(ClientError::Unexpected(format!("{other:?}"))),
        }
    }

    pub fn shutdown(mut self) -> Result<()> {
        self.send(&Request::Shutdown)?;
        match self.read()? {
            Response::Bye => {
                self.finished = true;
                let _ = self.child.wait();
                Ok(())
            }
            Response::Error { code, message } => Err(ClientError::Worker { code, message }),
            other => Err(ClientError::Unexpected(format!("{other:?}"))),
        }
    }

    /// Kill the worker immediately. Safe to call from a watchdog thread when
    /// the handler has exclusive ownership elsewhere via a process handle.
    pub fn kill(&mut self) {
        self.finished = true;
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    fn expect_summary(&mut self, request: Request) -> Result<Summary> {
        self.send(&request)?;
        match self.read()? {
            Response::Summary { summary } => Ok(summary),
            Response::Error { code, message } => Err(ClientError::Worker { code, message }),
            other => Err(ClientError::Unexpected(format!("{other:?}"))),
        }
    }

    fn send(&mut self, request: &Request) -> Result<()> {
        write_control(&mut self.input, request)?;
        Ok(())
    }

    fn read(&mut self) -> Result<Response> {
        let frame = read_frame(&mut self.output, &self.frames)?.ok_or(ClientError::Closed)?;
        match frame {
            Frame::Control(_) => Ok(decode_control(frame)?),
            Frame::StreamChunk(_) => Err(ClientError::Unexpected(
                "unexpected stream chunk from worker".to_string(),
            )),
        }
    }
}

impl Drop for WorkerClient {
    fn drop(&mut self) {
        if !self.finished {
            self.finished = true;
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// Wraps a reader so a long stream transfer can be interrupted between chunks.
///
/// The preview loop wraps the host `IStream` reader with this; when the flag is
/// set, the next `read` fails and `WorkerClient::open_stream` returns an error
/// instead of continuing. The protocol is unchanged, and bounded memory is
/// preserved: only the chunk buffer is live at any time.
pub struct CancelReader<'a> {
    inner: &'a mut dyn Read,
    cancel: &'a AtomicBool,
}

impl<'a> CancelReader<'a> {
    pub fn new(inner: &'a mut dyn Read, cancel: &'a AtomicBool) -> Self {
        Self { inner, cancel }
    }
}

impl Read for CancelReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "the preview was cancelled",
            ));
        }
        self.inner.read(buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn cancel_reader_passes_through_until_cancelled() {
        let mut source = Cursor::new(vec![7u8; 256 * 1024]);
        let cancel = AtomicBool::new(false);
        let mut reader = CancelReader::new(&mut source, &cancel);
        let mut sink = Vec::new();
        std::io::copy(&mut reader, &mut sink).unwrap();
        assert_eq!(sink.len(), 256 * 1024);
        assert!(sink.iter().all(|byte| *byte == 7));

        // Once cancellation is requested the next read fails rather than
        // returning more of the stream.
        cancel.store(true, Ordering::Relaxed);
        let mut buffer = [0u8; 16];
        assert_eq!(
            reader.read(&mut buffer).unwrap_err().kind(),
            std::io::ErrorKind::Interrupted
        );
    }
}

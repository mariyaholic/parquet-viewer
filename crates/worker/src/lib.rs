// SPDX-License-Identifier: Apache-2.0
//! Isolated worker: versioned protocol, bounded spool, read-only inspection.

pub mod spool;
mod stream_receipt;

use std::io::{BufReader, Read, Write};

use parquet_preview_core::about::about_text;
use parquet_preview_core::{Limits, PreviewError, Session, Settings};
use parquet_preview_protocol::{
    bounded_message, read_frame, write_control, ErrorCode, Frame, FrameLimits, Request, Response,
    DEFAULT_MAX_CONTROL_FRAME, DEFAULT_MAX_STREAM_CHUNK, PROTOCOL_VERSION,
};
use spool::SpoolFile;

#[derive(Default)]
pub struct WorkerConfig {
    pub limits: Limits,
    pub settings: Settings,
    pub frame_limits: FrameLimits,
}

struct OpenDocument {
    // Field order matters: the session (and its file handle) must drop before
    // the spool removes the path, especially on Windows.
    session: Session,
    _spool: SpoolFile,
}

struct State {
    limits: Limits,
    settings: Settings,
    hello: bool,
    stream: Option<SpoolFile>,
    declared: u64,
    received: u64,
    document: Option<OpenDocument>,
}

enum Outcome {
    Continue,
    Close,
}

/// Serve one worker connection until shutdown, EOF or a fatal protocol error.
pub fn serve<R: Read, W: Write>(
    input: R,
    mut output: W,
    config: WorkerConfig,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(input);
    let frames = config.frame_limits;
    let mut state = State {
        limits: config.limits,
        settings: config.settings,
        hello: false,
        stream: None,
        declared: 0,
        received: 0,
        document: None,
    };

    loop {
        let frame = match read_frame(&mut reader, &frames) {
            Ok(Some(frame)) => frame,
            Ok(None) => return Ok(()),
            Err(error) => {
                // The stream is no longer trustworthy; close without trying to
                // resynchronize.
                eprintln!("parquet-preview-worker: protocol frame error: {error}");
                return Ok(());
            }
        };
        match frame {
            Frame::StreamChunk(chunk) => {
                let before = state.received;
                match state.accept_chunk(&chunk) {
                    Ok(()) => {
                        if stream_receipt::newly_complete(before, state.received, state.declared) {
                            write_control(
                                &mut output,
                                &Response::StreamAccepted {
                                    bytes: state.received,
                                },
                            )
                            .map_err(protocol_write_error)?;
                        }
                    }
                    Err((code, message)) => {
                        write_control(&mut output, &Response::Error { code, message })
                            .map_err(protocol_write_error)?;
                        return Ok(());
                    }
                }
            }
            Frame::Control(payload) => {
                let request: Request = match serde_json::from_slice(&payload) {
                    Ok(request) => request,
                    Err(error) => {
                        write_control(
                            &mut output,
                            &Response::Error {
                                code: ErrorCode::ProtocolError,
                                message: bounded_message(&error.to_string()),
                            },
                        )
                        .map_err(protocol_write_error)?;
                        return Ok(());
                    }
                };
                match state.handle(request, &mut output, &frames) {
                    Ok(Outcome::Continue) => {}
                    Ok(Outcome::Close) => return Ok(()),
                    Err(error) => {
                        return Err(protocol_write_error(error));
                    }
                }
            }
        }
    }
}

// A partial response is terminal: the peer cannot safely find the next frame.
// Keep the original I/O kind when available; serialization/cap failures also
// terminate rather than attempting to continue on an unacknowledged request.
fn protocol_write_error(error: parquet_preview_protocol::ProtocolError) -> std::io::Error {
    match error {
        parquet_preview_protocol::ProtocolError::Io(error) => error,
        error => std::io::Error::other(error),
    }
}

impl State {
    fn accept_chunk(&mut self, chunk: &[u8]) -> Result<(), (ErrorCode, String)> {
        if !self.hello {
            return Err((
                ErrorCode::ProtocolError,
                "stream chunk before the Hello handshake".to_string(),
            ));
        }
        let Some(stream) = self.stream.as_mut() else {
            return Err((
                ErrorCode::ProtocolError,
                "stream chunk without OpenStream".to_string(),
            ));
        };
        let next = self.received.saturating_add(chunk.len() as u64);
        if next > self.declared {
            return Err((
                ErrorCode::StreamTooLarge,
                format!("stream exceeded its declared {}-byte size", self.declared),
            ));
        }
        stream
            .append(chunk)
            .map_err(|error| (ErrorCode::StreamTooLarge, worker_message(&error)))?;
        self.received = next;
        Ok(())
    }

    fn handle<W: Write>(
        &mut self,
        request: Request,
        output: &mut W,
        frames: &FrameLimits,
    ) -> Result<Outcome, parquet_preview_protocol::ProtocolError> {
        if !self.hello {
            match request {
                Request::Hello {
                    protocol_version,
                    client,
                } => {
                    let _ = client;
                    if protocol_version != PROTOCOL_VERSION {
                        write_control(
                            output,
                            &Response::Error {
                                code: ErrorCode::ProtocolMismatch,
                                message: format!(
                                    "worker protocol {PROTOCOL_VERSION}; client protocol {protocol_version}"
                                ),
                            },
                        )?;
                        write_control(output, &Response::Bye)?;
                        return Ok(Outcome::Close);
                    }
                    self.hello = true;
                    write_control(
                        output,
                        &Response::Welcome {
                            worker_version: env!("CARGO_PKG_VERSION").to_string(),
                            protocol_version: PROTOCOL_VERSION,
                            max_file_bytes: self.limits.max_file_bytes,
                            max_control_frame: frames.max_control as u64,
                            max_stream_chunk: frames.max_stream_chunk as u64,
                        },
                    )?;
                    return Ok(Outcome::Continue);
                }
                _ => {
                    write_control(
                        output,
                        &Response::Error {
                            code: ErrorCode::ProtocolError,
                            message: "the first request must be Hello".to_string(),
                        },
                    )?;
                    return Ok(Outcome::Close);
                }
            }
        }

        match request {
            Request::Hello { .. } => {
                write_control(
                    output,
                    &Response::Error {
                        code: ErrorCode::ProtocolError,
                        message: "Hello was already sent".to_string(),
                    },
                )?;
                Ok(Outcome::Continue)
            }
            Request::OpenStream { declared_size } => {
                self.document = None;
                self.stream = None;
                self.received = 0;
                self.declared = declared_size;
                if declared_size > self.limits.max_file_bytes {
                    write_control(
                        output,
                        &Response::Error {
                            code: ErrorCode::StreamTooLarge,
                            message: format!(
                                "declared size {declared_size} exceeds the {}-byte limit",
                                self.limits.max_file_bytes
                            ),
                        },
                    )?;
                    return Ok(Outcome::Continue);
                }
                if declared_size == 0 {
                    write_control(
                        output,
                        &Response::Error {
                            code: ErrorCode::FileError,
                            message: "the supplied stream is empty".into(),
                        },
                    )?;
                    return Ok(Outcome::Continue);
                }
                match SpoolFile::new(self.limits.max_file_bytes) {
                    Ok(spool) => {
                        self.stream = Some(spool);
                    }
                    Err(error) => {
                        write_control(
                            output,
                            &Response::Error {
                                code: ErrorCode::Internal,
                                message: worker_message(&error),
                            },
                        )?;
                    }
                }
                Ok(Outcome::Continue)
            }
            Request::StreamAbort => {
                self.stream = None;
                self.received = 0;
                self.declared = 0;
                write_control(output, &Response::StreamAborted)?;
                Ok(Outcome::Continue)
            }
            Request::Inspect => {
                if self.document.is_none() {
                    if self.stream.is_some() && self.received < self.declared {
                        write_control(
                            output,
                            &Response::Error {
                                code: ErrorCode::StreamIncomplete,
                                message: format!(
                                    "stream incomplete: {} of {} bytes received",
                                    self.received, self.declared
                                ),
                            },
                        )?;
                        return Ok(Outcome::Continue);
                    }
                    match self.open_document() {
                        Ok(Some(summary)) => {
                            write_control(output, &Response::Summary { summary })?;
                        }
                        Ok(None) => {
                            write_control(
                                output,
                                &Response::Error {
                                    code: ErrorCode::NoFile,
                                    message: "no inspected stream; send OpenStream, data and Inspect first"
                                        .to_string(),
                                },
                            )?;
                        }
                        Err(error) => {
                            write_control(
                                output,
                                &Response::Error {
                                    code: ErrorCode::FileError,
                                    message: worker_message(&error),
                                },
                            )?;
                        }
                    }
                } else if let Some(document) = &self.document {
                    write_control(
                        output,
                        &Response::Summary {
                            summary: document.session.summary().clone(),
                        },
                    )?;
                }
                Ok(Outcome::Continue)
            }
            Request::Sample { offset, limit } => {
                let Some(document) = &self.document else {
                    return self.no_file(output);
                };
                match document.session.sample(offset, limit as usize) {
                    Ok(sample) => write_control(output, &Response::Sample { sample })?,
                    Err(error) => self.reply_error(output, ErrorCode::Internal, &error)?,
                }
                Ok(Outcome::Continue)
            }
            Request::Scan => {
                let Some(document) = &self.document else {
                    return self.no_file(output);
                };
                let cancel = parquet_preview_core::CancelToken::new();
                match document.session.scan(&cancel) {
                    Ok(scan) => write_control(output, &Response::Scan { scan })?,
                    Err(error) => self.reply_error(output, ErrorCode::Internal, &error)?,
                }
                Ok(Outcome::Continue)
            }
            Request::Diagnose {
                market_profile,
                baseline,
                rows,
            } => {
                let Some(document) = &self.document else {
                    return self.no_file(output);
                };
                let profile = market_profile.or_else(|| self.settings.market_profile.clone());
                let baseline = baseline.or_else(|| self.settings.baseline.clone());
                let Some(profile) = profile else {
                    write_control(
                        output,
                        &Response::Error {
                            code: ErrorCode::ProfileMissing,
                            message: "no market profile configured; diagnostics are opt-in"
                                .to_string(),
                        },
                    )?;
                    return Ok(Outcome::Continue);
                };
                let cancel = parquet_preview_core::CancelToken::new();
                match document.session.diagnose(
                    &profile,
                    baseline.as_ref(),
                    rows.map(|v| v as usize).unwrap_or(50_000),
                    &cancel,
                ) {
                    Ok(market) => write_control(output, &Response::Market { market })?,
                    Err(error) => self.reply_error(output, ErrorCode::Internal, &error)?,
                }
                Ok(Outcome::Continue)
            }
            Request::Render { mut options } => {
                let Some(document) = &self.document else {
                    return self.no_file(output);
                };
                if options.market_profile.is_none() {
                    options.market_profile = self.settings.market_profile.clone();
                }
                if options.baseline.is_none() {
                    options.baseline = self.settings.baseline.clone();
                }
                match document.session.preview(&options) {
                    Ok(report) => {
                        let text = parquet_preview_core::report::render(&report);
                        let response = Response::Preview { report, text };
                        if !self.try_write_response(output, &response)? {
                            write_control(
                                output,
                                &Response::Error {
                                    code: ErrorCode::Internal,
                                    message:
                                        "preview response exceeded the control-frame cap; request fewer sample rows"
                                            .to_string(),
                                },
                            )?;
                        }
                    }
                    Err(error) => self.reply_error(output, ErrorCode::Internal, &error)?,
                }
                Ok(Outcome::Continue)
            }
            Request::About => {
                write_control(output, &Response::About { text: about_text() })?;
                Ok(Outcome::Continue)
            }
            Request::Shutdown => {
                write_control(output, &Response::Bye)?;
                Ok(Outcome::Close)
            }
        }
    }

    fn open_document(
        &mut self,
    ) -> Result<Option<parquet_preview_core::summary::Summary>, PreviewError> {
        if self.stream.is_none() {
            return Ok(None);
        }
        if self.received != self.declared {
            return Err(PreviewError::Stream(format!(
                "stream incomplete: {} of {} bytes received",
                self.received, self.declared
            )));
        }
        let mut spool = self.stream.take().expect("stream checked above");
        let file = spool.take_file()?;
        let session = Session::open(Box::new(file), &self.limits)?;
        let summary = session.summary().clone();
        self.document = Some(OpenDocument {
            session,
            _spool: spool,
        });
        Ok(Some(summary))
    }

    fn no_file<W: Write>(
        &self,
        output: &mut W,
    ) -> Result<Outcome, parquet_preview_protocol::ProtocolError> {
        write_control(
            output,
            &Response::Error {
                code: ErrorCode::NoFile,
                message: "no inspected stream; send OpenStream, data and Inspect first".to_string(),
            },
        )?;
        Ok(Outcome::Continue)
    }

    fn reply_error<W: Write>(
        &self,
        output: &mut W,
        code: ErrorCode,
        error: &PreviewError,
    ) -> Result<(), parquet_preview_protocol::ProtocolError> {
        write_control(
            output,
            &Response::Error {
                code,
                message: worker_message(error),
            },
        )
    }

    /// Serialize a response first so an oversized one cannot poison the writer.
    fn try_write_response<W: Write>(
        &self,
        output: &mut W,
        response: &Response,
    ) -> Result<bool, parquet_preview_protocol::ProtocolError> {
        let payload = match serde_json::to_vec(response) {
            Ok(payload) => payload,
            Err(_) => return Ok(false),
        };
        if payload.len() > DEFAULT_MAX_CONTROL_FRAME {
            return Ok(false);
        }
        // Use the public writer; the payload is known to fit.
        write_control(output, response)?;
        Ok(true)
    }
}

fn worker_message(error: &PreviewError) -> String {
    bounded_message(&error.to_string())
}

/// Print the version and protocol pair for packaging checks.
pub fn version_line() -> String {
    format!(
        "parquet-preview-worker {} protocol {} (max control {} bytes, max chunk {} bytes)",
        env!("CARGO_PKG_VERSION"),
        PROTOCOL_VERSION,
        DEFAULT_MAX_CONTROL_FRAME,
        DEFAULT_MAX_STREAM_CHUNK
    )
}

# Worker protocol (v1)

Status: implemented in `crates/protocol` and `crates/worker`.

## Transport

The handler starts `parquet-preview-worker.exe` with piped stdin/stdout. Stderr
is discarded by the handler. Frames are:

```text
u32 big-endian payload length | u8 tag | payload
```

- Tag `1` (`control`): UTF-8 JSON request/response. Cap: 8 MiB.
- Tag `2` (`stream`): raw bytes of the caller-supplied stream. Cap: 256 KiB per
  frame.

The protocol version is `1` and is checked by the `Hello` handshake before any
stream is accepted. A mismatch returns `error(protocol_mismatch)` followed by
`bye`, and the connection closes. Frame errors close the worker; the host
kills the process on selection changes.

## Sequence

```text
-> hello { protocol_version: 1 }
<- welcome { worker_version, protocol_version, max_file_bytes, ... }

-> open_stream { declared_size }
-> stream chunks (raw frames)
<- stream_accepted { bytes }        (once declared_size bytes arrive)
-> inspect
<- summary { ... }

-> sample { offset, limit }
<- sample { ... }

-> render { options }
<- preview { report, text }

-> shutdown
<- bye
```

`stream_abort` discards an in-progress stream and returns `stream_aborted`.
`inspect` without a complete stream returns `error(stream_incomplete)` or
`error(no_file)`. The worker accepts a new `open_stream` and replaces the
previous document.

A worker can also answer `scan`, `diagnose`, `about`, and `error`. Responses
include the exact preview report and bounded text used by the preview pane.

## Bounds

- Declared size above `max_file_bytes` is rejected.
- Chunks beyond the declared size are fatal (`stream_too_large`).
- Bytes are spooled to a new file in the per-user temporary directory with
  owner-only permissions where the platform supports them; the path is removed
  when the document closes.
- Control responses are serialized before writing. If a `preview` response would
  exceed the control cap, the worker returns `error(internal)` instead of a
  partial frame.
- The worker never opens a path supplied by the handler. The handler streams the
  host's `IStream`; the worker owns and deletes its spool.
- Cancellation is by process termination (job object or `TerminateProcess`);
  the core also has its own sample/scan/diagnostic budgets.

## Error codes

`protocol_mismatch`, `protocol_error`, `stream_too_large`, `stream_incomplete`,
`no_file`, `file_error`, `profile_missing`, `internal`.

Messages are bounded to 500 characters and must not contain credentials.

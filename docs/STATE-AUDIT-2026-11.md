# State-machine review follow-up

Base: `47f28ac169bd522a35b0fe4bdb0e912eb984e2da`. Original themes, material contracts and assets are unchanged. No shelved theme work is included.

## Corrections

StreamAccepted is emitted only on the transition from incomplete to complete. Empty frames after a completed upload no longer repeat acknowledgement and desynchronize later replies. The wire format and supported empty-frame behavior are unchanged. An in-memory worker protocol regression sends 10,000 empty frames after completion and requires exactly one acceptance followed by About and Bye.

The pure transition stress covers 1,000 stream sizes and approximately two million decisions. The existing viewer-shell cancellation tests also wait for asynchronous process/slot cleanup while retaining the shared test lock, mirroring CSV's regression correction. Production supervisor behavior is unchanged.

## Verification and limits

All 168 tracked blobs from pinned main, including the formerly unavailable binary fonts, images and Parquet fixtures, are restored and hash-verified for integration. Formatting, generated documentation, six documentation tests, 18 design tests and original theme-token parity pass locally, as does the actual transition helper stress test.

The original worker audit could not run the full Arrow-dependent suite. This PR adds a full Linux workspace job alongside the existing native Windows build/tests, standalone control/paging smoke and opt-in COM end-to-end harness. Only the exact-head CI result establishes those stages as passed. The local host lacks a Windows resource compiler/linker; Wine aborts at prefix initialization before application launch. No real Explorer installation or broad interactive hardware/display acceptance is claimed.

The existing STATE-MACHINES document remains a subsystem inventory, not proof of all interleavings. No merge, release or deployment is performed.


## Response-write failures

Any failed protocol response write now terminates serve with an I/O error,
including StreamAccepted, chunk rejection, malformed-control replies, and
ordinary request responses. Fault-injecting tests fail before or inside the
chunk-response frame and assert that no further request is read. Malformed
incoming framing still closes without attempting resynchronization; this is
separate from a failed outgoing response.


## Additional review corrections

Shared-temp sweeping is disabled. Name and age never establish ownership or
worker liveness. Existing Windows delete-on-close and owned SpoolFile cleanup
remain; abandoned files from older builds need user-directed cleanup.

Settings reads return defaults only for NotFound, read at most 1 MiB plus the
overflow byte, and surface all other read/parse/validation errors. Worker
startup sends a protocol error and exits instead of silently using defaults.
Settings saves stage in an exclusive same-directory file, sync, close and
rename without truncating the destination. Failed preparation/publication
preserves prior bytes. Concurrent saves remain last-writer-wins; directory
sync/power-loss durability and external modifications are not guaranteed.

## Handle-owned spool lifetime

Spool Drop no longer deletes a pathname after closing the owned file. Windows
relies only on delete-on-close for the owned object. Linux creates an anonymous
O_TMPFILE inode and refuses filesystems that do not support it, rather than
falling back to a shared-temp deletion race. Other operating systems fail with
Unsupported. The diagnostic Linux name is not a directory entry. Six tests of
the actual spool module passed on Linux, including preservation of a replacement
at the old name and active/unrelated retention. Final native tests remain in CI.

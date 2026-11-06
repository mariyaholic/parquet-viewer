# State-machine review follow-up

Base: `47f28ac169bd522a35b0fe4bdb0e912eb984e2da`. Original themes, material contracts and assets are unchanged. No shelved theme work is included.

## Corrections

StreamAccepted is emitted only on the transition from incomplete to complete. Empty frames after a completed upload no longer repeat acknowledgement and desynchronize later replies. The wire format and supported empty-frame behavior are unchanged. An in-memory worker protocol regression sends 10,000 empty frames after completion and requires exactly one acceptance followed by About and Bye.

The pure transition stress covers 1,000 stream sizes and approximately two million decisions. The existing viewer-shell cancellation tests also wait for asynchronous process/slot cleanup while retaining the shared test lock, mirroring CSV's regression correction. Production supervisor behavior is unchanged.

## Verification and limits

All 168 tracked blobs from pinned main, including the formerly unavailable binary fonts, images and Parquet fixtures, are restored and hash-verified for integration. Formatting, generated documentation, six documentation tests, 18 design tests and original theme-token parity pass locally, as does the actual transition helper stress test.

The original worker audit could not run the full Arrow-dependent suite. This PR adds a full Linux workspace job alongside the existing native Windows build/tests, standalone control/paging smoke and opt-in COM end-to-end harness. Only the exact-head CI result establishes those stages as passed. The local host lacks a Windows resource compiler/linker; Wine aborts at prefix initialization before application launch. No real Explorer installation or broad interactive hardware/display acceptance is claimed.

The existing STATE-MACHINES document remains a subsystem inventory, not proof of all interleavings. No merge, release or deployment is performed.

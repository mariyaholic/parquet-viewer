// SPDX-License-Identifier: Apache-2.0
//! Windows COM preview handler and per-user registration.
//!
//! The COM and Win32 modules are compiled on Windows and can be type-checked on
//! Linux with `--features win-check`. Registration logic is platform-neutral and
//! unit-tested everywhere.

pub mod registration;

#[cfg(any(windows, feature = "win-check"))]
mod com;
#[cfg(any(windows, feature = "win-check"))]
mod ffi;
#[cfg(any(windows, feature = "win-check"))]
mod handler;
#[cfg(any(windows, feature = "win-check"))]
mod job;
#[cfg(any(windows, feature = "win-check"))]
mod stream;
#[cfg(any(windows, feature = "win-check"))]
mod win32_registry;

#[cfg(all(windows, not(target_pointer_width = "64")))]
compile_error!("the v1 preview handler targets 64-bit Windows only");

pub use parquet_preview_core::about::{about_text, MADE_BY, STORE_COPY};

pub use registration::{
    clsid_key_name, inproc_key_name, register, shell_ex_key_name, unregister, RegistrationError,
    RegistrationReport, UnregistrationReport, CLSID, EXTENSION, PREVIEW_HANDLER_SHELLEXT,
};

/// About text shown by the handler's F1 dialog.
pub fn about_screen_text() -> String {
    about_text()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn about_screen_credits_maria() {
        let text = about_screen_text();
        assert!(text.contains(MADE_BY));
        assert!(text.contains(STORE_COPY));
    }
}

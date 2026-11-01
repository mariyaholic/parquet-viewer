// SPDX-License-Identifier: Apache-2.0
//! Read-only adapter over the host-supplied `IStream`.
//!
//! The COM stream is never assumed to be a file. Its size comes from `Stat`;
//! bytes are read in bounded chunks and forwarded to the worker protocol.

use std::ffi::c_void;
use std::io::{self, Read};

use crate::ffi;

#[repr(C)]
struct IStreamVtbl {
    query_interface:
        unsafe extern "system" fn(*mut c_void, *const ffi::GUID, *mut *mut c_void) -> ffi::HRESULT,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    read: unsafe extern "system" fn(*mut c_void, *mut u8, u32, *mut u32) -> ffi::HRESULT,
    write: unsafe extern "system" fn(*mut c_void, *const u8, u32, *mut u32) -> ffi::HRESULT,
    seek: unsafe extern "system" fn(*mut c_void, i64, u32, *mut u64) -> ffi::HRESULT,
    set_size: unsafe extern "system" fn(*mut c_void, u64) -> ffi::HRESULT,
    copy_to: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        u64,
        *mut u64,
        *mut u64,
    ) -> ffi::HRESULT,
    commit: unsafe extern "system" fn(*mut c_void, u32) -> ffi::HRESULT,
    revert: unsafe extern "system" fn(*mut c_void) -> ffi::HRESULT,
    lock_region: unsafe extern "system" fn(*mut c_void, u64, u64, u32) -> ffi::HRESULT,
    unlock_region: unsafe extern "system" fn(*mut c_void, u64, u64, u32) -> ffi::HRESULT,
    stat: unsafe extern "system" fn(*mut c_void, *mut ffi::STATSTG, u32) -> ffi::HRESULT,
    clone: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> ffi::HRESULT,
}

unsafe fn vtable(stream: *mut c_void) -> &'static IStreamVtbl {
    let table = *(stream as *const *const IStreamVtbl);
    &*table
}

pub unsafe fn add_ref(stream: *mut c_void) -> u32 {
    (vtable(stream).add_ref)(stream)
}

pub unsafe fn release(stream: *mut c_void) -> u32 {
    (vtable(stream).release)(stream)
}

pub unsafe fn seek_to_start(stream: *mut c_void) -> Result<(), String> {
    let mut new_position = 0u64;
    let hr = (vtable(stream).seek)(stream, 0, ffi::STREAM_SEEK_SET, &mut new_position);
    if hr < 0 {
        return Err(format!("IStream::Seek failed (0x{hr:08X})"));
    }
    Ok(())
}

pub unsafe fn stream_size(stream: *mut c_void) -> Result<u64, String> {
    let mut stat = ffi::STATSTG {
        pwcsName: std::ptr::null_mut(),
        type_: 0,
        cbSize: 0,
        mtime: ffi::FILETIME::default(),
        ctime: ffi::FILETIME::default(),
        atime: ffi::FILETIME::default(),
        grfMode: 0,
        grfLocksSupported: 0,
        clsid: ffi::GUID::from_u128(0),
        grfStateBits: 0,
        reserved: 0,
    };
    let hr = (vtable(stream).stat)(stream, &mut stat, ffi::STATFLAG_NONAME);
    if hr < 0 {
        return Err(format!("IStream::Stat failed (0x{hr:08X})"));
    }
    if stat.cbSize == 0 {
        return Err("the supplied stream does not report a size".to_string());
    }
    Ok(stat.cbSize)
}

/// Marshal the stream so another thread can read it without copying the file
/// into memory. Must be called on a COM-initialized thread that owns a
/// reference to `stream`. Returns a marshalled `IStream` pointer; pass it to
/// `unmarshal` on the destination thread, which releases it.
pub unsafe fn marshal(stream: *mut c_void) -> Result<*mut c_void, String> {
    let mut marshalled: *mut c_void = std::ptr::null_mut();
    let hr = ffi::CoMarshalInterThreadInterfaceInStream(&ffi::IID_ISTREAM, stream, &mut marshalled);
    if hr < 0 {
        return Err(format!(
            "CoMarshalInterThreadInterfaceInStream failed (0x{hr:08X})"
        ));
    }
    Ok(marshalled)
}

/// A stream unmarshalled onto this thread, together with the COM apartment it
/// was created in. Dropping it releases the stream and tears the apartment
/// down.
pub struct UnmarshalledStream {
    stream: *mut c_void,
}

impl UnmarshalledStream {
    pub fn as_ptr(&self) -> *mut c_void {
        self.stream
    }
}

impl Drop for UnmarshalledStream {
    fn drop(&mut self) {
        unsafe {
            release(self.stream);
            ffi::CoUninitialize();
        }
    }
}

/// Unmarshal a stream produced by `marshal` onto the calling thread.
///
/// `CoInitializeEx` is called first (the apartment is torn down by the returned
/// value's `Drop`). The marshalled stream is consumed either way, so a failure
/// here does not leak it.
pub unsafe fn unmarshal(marshalled: *mut c_void) -> Result<UnmarshalledStream, String> {
    let hr = ffi::CoInitializeEx(std::ptr::null_mut(), ffi::COINIT_MULTITHREADED);
    if hr < 0 {
        // `unmarshal` never got the chance to consume the stream; release it.
        release(marshalled);
        return Err(format!("CoInitializeEx failed (0x{hr:08X})"));
    }
    let mut stream: *mut c_void = std::ptr::null_mut();
    let hr = ffi::CoGetInterfaceAndReleaseStream(marshalled, &ffi::IID_ISTREAM, &mut stream);
    if hr < 0 {
        // CoGetInterfaceAndReleaseStream released the marshalled stream.
        ffi::CoUninitialize();
        return Err(format!(
            "CoGetInterfaceAndReleaseStream failed (0x{hr:08X})"
        ));
    }
    Ok(UnmarshalledStream { stream })
}

/// Read adapter used only on the owning COM apartment while the handler
/// owns a reference to the stream.
pub struct StreamReader {
    stream: *mut c_void,
}

impl StreamReader {
    /// # Safety
    /// `stream` must be a live `IStream` pointer with an outstanding reference.
    pub unsafe fn new(stream: *mut c_void) -> Self {
        Self { stream }
    }
}

impl Read for StreamReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        let mut read = 0u32;
        let hr = unsafe {
            (vtable(self.stream).read)(
                self.stream,
                buffer.as_mut_ptr(),
                buffer.len().min(u32::MAX as usize) as u32,
                &mut read,
            )
        };
        if hr < 0 {
            return Err(io::Error::other(format!(
                "IStream::Read failed (0x{hr:08X})"
            )));
        }
        Ok(read as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[link(name = "shlwapi")]
    extern "system" {
        fn SHCreateMemStream(init: *const u8, size: u32) -> *mut c_void;
    }

    #[test]
    fn guid_bytes_match_iunknown_layout() {
        let guid = ffi::IID_IUNKNOWN;
        assert_eq!(
            guid.to_registry_string(),
            "{00000000-0000-0000-C000-000000000046}"
        );
    }

    /// The marshalling contract M1 relies on: a stream created on one thread is
    /// readable on another after marshal/unmarshal, and the marshalled stream is
    /// consumed exactly once.
    #[test]
    fn a_stream_marshals_across_threads() {
        let payload: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
        let stream = unsafe { SHCreateMemStream(payload.as_ptr(), payload.len() as u32) };
        assert!(!stream.is_null(), "SHCreateMemStream failed");

        // The owning thread must be COM-initialized before marshalling.
        let hr =
            unsafe { ffi::CoInitializeEx(std::ptr::null_mut(), ffi::COINIT_APARTMENTTHREADED) };
        assert!(hr >= 0, "CoInitializeEx failed (0x{hr:08X})");
        let marshalled = unsafe { marshal(stream) }.expect("marshal the stream");
        // A raw pointer is not `Send`; it is safe here because the marshalled
        // stream is explicitly designed to move between apartments.
        let address = marshalled as usize;

        let handle = std::thread::spawn(move || {
            let unmarshalled = unsafe { unmarshal(address as *mut c_void) }.expect("unmarshal");
            let mut reader = unsafe { StreamReader::new(unmarshalled.as_ptr()) };
            let mut out = Vec::new();
            reader.read_to_end(&mut out).unwrap();
            out
        });
        let out = handle.join().unwrap();
        assert_eq!(out, payload);

        unsafe {
            release(stream);
            ffi::CoUninitialize();
        }
    }
}

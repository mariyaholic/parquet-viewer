// SPDX-License-Identifier: Apache-2.0
//! COM preview handler object, class factory and DLL exports.

#![allow(non_snake_case)]

use std::ffi::c_void;
use std::mem::offset_of;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use crate::ffi;
use crate::handler::HandlerState;
use crate::registration::{register, unregister};
use crate::win32_registry::{self, Win32Registry};

#[repr(C)]
struct IUnknownIfc {
    vtbl: *const IUnknownVtbl,
}

#[repr(C)]
struct IInitializeWithStreamIfc {
    vtbl: *const IInitializeWithStreamVtbl,
}

#[repr(C)]
struct IPreviewHandlerIfc {
    vtbl: *const IPreviewHandlerVtbl,
}

#[repr(C)]
struct IPreviewHandlerVisualsIfc {
    vtbl: *const IPreviewHandlerVisualsVtbl,
}

#[repr(C)]
struct IObjectWithSiteIfc {
    vtbl: *const IObjectWithSiteVtbl,
}

#[repr(C)]
struct IUnknownVtbl {
    query_interface:
        unsafe extern "system" fn(*mut c_void, *const ffi::GUID, *mut *mut c_void) -> ffi::HRESULT,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
}

#[repr(C)]
struct IInitializeWithStreamVtbl {
    query_interface:
        unsafe extern "system" fn(*mut c_void, *const ffi::GUID, *mut *mut c_void) -> ffi::HRESULT,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    initialize: unsafe extern "system" fn(*mut c_void, *mut c_void, u32) -> ffi::HRESULT,
}

#[repr(C)]
struct IPreviewHandlerVtbl {
    query_interface:
        unsafe extern "system" fn(*mut c_void, *const ffi::GUID, *mut *mut c_void) -> ffi::HRESULT,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    set_window: unsafe extern "system" fn(*mut c_void, ffi::HWND, *const ffi::RECT) -> ffi::HRESULT,
    set_rect: unsafe extern "system" fn(*mut c_void, *const ffi::RECT) -> ffi::HRESULT,
    do_preview: unsafe extern "system" fn(*mut c_void) -> ffi::HRESULT,
    unload: unsafe extern "system" fn(*mut c_void) -> ffi::HRESULT,
    set_focus: unsafe extern "system" fn(*mut c_void) -> ffi::HRESULT,
    query_focus: unsafe extern "system" fn(*mut c_void, *mut ffi::HWND) -> ffi::HRESULT,
    translate_accelerator: unsafe extern "system" fn(*mut c_void, *const ffi::MSG) -> ffi::HRESULT,
}

#[repr(C)]
struct IPreviewHandlerVisualsVtbl {
    query_interface:
        unsafe extern "system" fn(*mut c_void, *const ffi::GUID, *mut *mut c_void) -> ffi::HRESULT,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    set_background_color: unsafe extern "system" fn(*mut c_void, ffi::COLORREF) -> ffi::HRESULT,
    set_font: unsafe extern "system" fn(*mut c_void, ffi::HFONT) -> ffi::HRESULT,
    set_text_color: unsafe extern "system" fn(*mut c_void, ffi::COLORREF) -> ffi::HRESULT,
}

#[repr(C)]
struct IObjectWithSiteVtbl {
    query_interface:
        unsafe extern "system" fn(*mut c_void, *const ffi::GUID, *mut *mut c_void) -> ffi::HRESULT,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    set_site: unsafe extern "system" fn(*mut c_void, *mut c_void) -> ffi::HRESULT,
    get_site:
        unsafe extern "system" fn(*mut c_void, *const ffi::GUID, *mut *mut c_void) -> ffi::HRESULT,
}

/// The subset of `IPreviewHandlerFrame` the handler calls back into.
#[repr(C)]
struct IPreviewHandlerFrameVtbl {
    query_interface:
        unsafe extern "system" fn(*mut c_void, *const ffi::GUID, *mut *mut c_void) -> ffi::HRESULT,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    get_window_context: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> ffi::HRESULT,
    translate_accelerator: unsafe extern "system" fn(*mut c_void, *mut ffi::MSG) -> ffi::HRESULT,
}

#[repr(C)]
struct ComObject {
    // Each interface starts with its own vtable pointer. `unknown` must stay
    // first so an IUnknown pointer is also the allocation base.
    unknown: IUnknownIfc,
    initialize: IInitializeWithStreamIfc,
    preview: IPreviewHandlerIfc,
    visuals: IPreviewHandlerVisualsIfc,
    object_with_site: IObjectWithSiteIfc,
    refs: AtomicU32,
    state: Mutex<HandlerState>,
    /// The host's `IPreviewHandlerFrame`, queried out of the site in `SetSite`.
    /// An owned reference, released when the object is destroyed.
    frame: Mutex<Option<*mut c_void>>,
}

static UNKNOWN_VTBL: IUnknownVtbl = IUnknownVtbl {
    query_interface: unknown_qi,
    add_ref: unknown_add_ref,
    release: unknown_release,
};

static INITIALIZE_VTBL: IInitializeWithStreamVtbl = IInitializeWithStreamVtbl {
    query_interface: initialize_qi,
    add_ref: initialize_add_ref,
    release: initialize_release,
    initialize: initialize_stream,
};

static PREVIEW_VTBL: IPreviewHandlerVtbl = IPreviewHandlerVtbl {
    query_interface: preview_qi,
    add_ref: preview_add_ref,
    release: preview_release,
    set_window,
    set_rect,
    do_preview,
    unload,
    set_focus,
    query_focus,
    translate_accelerator,
};

static VISUALS_VTBL: IPreviewHandlerVisualsVtbl = IPreviewHandlerVisualsVtbl {
    query_interface: visuals_qi,
    add_ref: visuals_add_ref,
    release: visuals_release,
    set_background_color: visuals_set_background_color,
    set_font: visuals_set_font,
    set_text_color: visuals_set_text_color,
};

static SITE_VTBL: IObjectWithSiteVtbl = IObjectWithSiteVtbl {
    query_interface: site_qi,
    add_ref: site_add_ref,
    release: site_release,
    set_site,
    get_site,
};

/// Call `release` through an arbitrary COM interface pointer.
unsafe fn release_interface(interface: *mut c_void) {
    if interface.is_null() {
        return;
    }
    let table = *(interface as *const *const IUnknownVtbl);
    ((*table).release)(interface);
}

/// Call `QueryInterface` through an arbitrary COM interface pointer. The caller
/// owns the returned reference when the call succeeds.
unsafe fn query_interface_ptr(
    interface: *mut c_void,
    iid: *const ffi::GUID,
    out: *mut *mut c_void,
) -> ffi::HRESULT {
    let table = *(interface as *const *const IUnknownVtbl);
    ((*table).query_interface)(interface, iid, out)
}

unsafe fn base_from(this: *mut c_void, offset: usize) -> *mut ComObject {
    (this as *mut u8).sub(offset) as *mut ComObject
}

unsafe fn add_ref(base: *mut ComObject) -> u32 {
    (*base).refs.fetch_add(1, Ordering::Relaxed) + 1
}

unsafe fn release(base: *mut ComObject) -> u32 {
    let previous = (*base).refs.fetch_sub(1, Ordering::AcqRel);
    if previous == 1 {
        // Release the host frame we hold before the object's state is dropped.
        if let Ok(mut frame) = (*base).frame.lock() {
            if let Some(frame) = frame.take() {
                release_interface(frame);
            }
        }
        drop(Box::from_raw(base));
        0
    } else {
        previous - 1
    }
}

unsafe fn query_interface(
    base: *mut ComObject,
    iid: *const ffi::GUID,
    out: *mut *mut c_void,
) -> ffi::HRESULT {
    if iid.is_null() || out.is_null() {
        return ffi::E_POINTER;
    }
    *out = std::ptr::null_mut();
    let iid = *iid;
    let candidate = if iid == ffi::IID_IUNKNOWN {
        &mut (*base).unknown as *mut IUnknownIfc as *mut c_void
    } else if iid == ffi::IID_IINITIALIZEWITHSTREAM {
        &mut (*base).initialize as *mut IInitializeWithStreamIfc as *mut c_void
    } else if iid == ffi::IID_IPREVIEWHANDLER {
        &mut (*base).preview as *mut IPreviewHandlerIfc as *mut c_void
    } else if iid == ffi::IID_IPREVIEWHANDLERVISUALS {
        &mut (*base).visuals as *mut IPreviewHandlerVisualsIfc as *mut c_void
    } else if iid == ffi::IID_IOBJECTWITHSITE {
        &mut (*base).object_with_site as *mut IObjectWithSiteIfc as *mut c_void
    } else {
        return ffi::E_NOINTERFACE;
    };
    add_ref(base);
    *out = candidate;
    ffi::S_OK
}

macro_rules! refcount_bridge {
    ($qi:ident, $add:ident, $rel:ident, $iface:ty, $field:ident) => {
        unsafe extern "system" fn $qi(
            this: *mut c_void,
            iid: *const ffi::GUID,
            out: *mut *mut c_void,
        ) -> ffi::HRESULT {
            query_interface(base_from(this, offset_of!(ComObject, $field)), iid, out)
        }

        unsafe extern "system" fn $add(this: *mut c_void) -> u32 {
            add_ref(base_from(this, offset_of!(ComObject, $field)))
        }

        unsafe extern "system" fn $rel(this: *mut c_void) -> u32 {
            release(base_from(this, offset_of!(ComObject, $field)))
        }
    };
}

refcount_bridge!(
    unknown_qi,
    unknown_add_ref,
    unknown_release,
    IUnknownIfc,
    unknown
);
refcount_bridge!(
    initialize_qi,
    initialize_add_ref,
    initialize_release,
    IInitializeWithStreamIfc,
    initialize
);
refcount_bridge!(
    preview_qi,
    preview_add_ref,
    preview_release,
    IPreviewHandlerIfc,
    preview
);
refcount_bridge!(
    visuals_qi,
    visuals_add_ref,
    visuals_release,
    IPreviewHandlerVisualsIfc,
    visuals
);
refcount_bridge!(
    site_qi,
    site_add_ref,
    site_release,
    IObjectWithSiteIfc,
    object_with_site
);

unsafe extern "system" fn initialize_stream(
    this: *mut c_void,
    stream: *mut c_void,
    mode: u32,
) -> ffi::HRESULT {
    let base = base_from(this, offset_of!(ComObject, initialize));
    let mut state = match (*base).state.lock() {
        Ok(state) => state,
        Err(_) => return ffi::E_FAIL,
    };
    state.initialize(stream, mode)
}

unsafe extern "system" fn set_window(
    this: *mut c_void,
    parent: ffi::HWND,
    rect: *const ffi::RECT,
) -> ffi::HRESULT {
    if rect.is_null() {
        return ffi::E_POINTER;
    }
    let base = base_from(this, offset_of!(ComObject, preview));
    let mut state = match (*base).state.lock() {
        Ok(state) => state,
        Err(_) => return ffi::E_FAIL,
    };
    match state.set_window(parent, &*rect) {
        Ok(()) => ffi::S_OK,
        Err(_) => ffi::E_FAIL,
    }
}

unsafe extern "system" fn set_rect(this: *mut c_void, rect: *const ffi::RECT) -> ffi::HRESULT {
    if rect.is_null() {
        return ffi::E_POINTER;
    }
    let base = base_from(this, offset_of!(ComObject, preview));
    let mut state = match (*base).state.lock() {
        Ok(state) => state,
        Err(_) => return ffi::E_FAIL,
    };
    state.set_rect(&*rect);
    ffi::S_OK
}

unsafe extern "system" fn do_preview(this: *mut c_void) -> ffi::HRESULT {
    let base = base_from(this, offset_of!(ComObject, preview));
    let mut state = match (*base).state.lock() {
        Ok(state) => state,
        Err(_) => return ffi::E_FAIL,
    };
    state.do_preview();
    ffi::S_OK
}

unsafe extern "system" fn unload(this: *mut c_void) -> ffi::HRESULT {
    let base = base_from(this, offset_of!(ComObject, preview));
    let mut state = match (*base).state.lock() {
        Ok(state) => state,
        Err(_) => return ffi::E_FAIL,
    };
    state.unload();
    ffi::S_OK
}

unsafe extern "system" fn set_focus(this: *mut c_void) -> ffi::HRESULT {
    let base = base_from(this, offset_of!(ComObject, preview));
    if let Ok(state) = (*base).state.lock() {
        state.set_focus();
        ffi::S_OK
    } else {
        ffi::E_FAIL
    }
}

unsafe extern "system" fn query_focus(this: *mut c_void, focus: *mut ffi::HWND) -> ffi::HRESULT {
    if focus.is_null() {
        return ffi::E_POINTER;
    }
    let base = base_from(this, offset_of!(ComObject, preview));
    if let Ok(state) = (*base).state.lock() {
        *focus = state.query_focus();
        ffi::S_OK
    } else {
        ffi::E_FAIL
    }
}

unsafe extern "system" fn translate_accelerator(
    this: *mut c_void,
    message: *const ffi::MSG,
) -> ffi::HRESULT {
    if message.is_null() {
        return ffi::E_POINTER;
    }
    let base = base_from(this, offset_of!(ComObject, preview));
    let handled = match (*base).state.lock() {
        Ok(state) => state.translate_accelerator(&*message),
        Err(_) => return ffi::E_FAIL,
    };
    if handled != ffi::S_FALSE {
        return handled;
    }
    // Unhandled keys (Tab and friends) belong to the host frame. Fail closed
    // when no site was supplied, so the shell keeps the key.
    let frame = match (*base).frame.lock() {
        Ok(frame) => *frame,
        Err(_) => return ffi::S_FALSE,
    };
    let Some(frame) = frame else {
        return ffi::S_FALSE;
    };
    let table = *(frame as *const *const IPreviewHandlerFrameVtbl);
    ((*table).translate_accelerator)(frame, message as *mut ffi::MSG)
}

/// `IObjectWithSite::SetSite` — store the host's site and query it for the
/// `IPreviewHandlerFrame` the accelerator forwarder calls back into.
unsafe extern "system" fn set_site(this: *mut c_void, site: *mut c_void) -> ffi::HRESULT {
    let base = base_from(this, offset_of!(ComObject, object_with_site));
    let mut slot = match (*base).frame.lock() {
        Ok(slot) => slot,
        Err(_) => return ffi::E_FAIL,
    };
    // Replace any previous site.
    if let Some(previous) = slot.take() {
        release_interface(previous);
    }
    if site.is_null() {
        // Explorer passes NULL to tear the site down.
        return ffi::S_OK;
    }
    let mut frame: *mut c_void = std::ptr::null_mut();
    let hr = query_interface_ptr(site, &ffi::IID_IPREVIEWHANDLERFRAME, &mut frame);
    if hr == ffi::S_OK && !frame.is_null() {
        *slot = Some(frame);
    }
    // Accept the site even when it exposes no frame: Explorer hands over
    // objects that may implement other interfaces too.
    ffi::S_OK
}

/// `IObjectWithSite::GetSite`.
unsafe extern "system" fn get_site(
    this: *mut c_void,
    iid: *const ffi::GUID,
    out: *mut *mut c_void,
) -> ffi::HRESULT {
    if iid.is_null() || out.is_null() {
        return ffi::E_POINTER;
    }
    *out = std::ptr::null_mut();
    let base = base_from(this, offset_of!(ComObject, object_with_site));
    let frame = match (*base).frame.lock() {
        Ok(slot) => *slot,
        Err(_) => return ffi::E_FAIL,
    };
    let Some(frame) = frame else {
        // No site: E_FAIL, as `IObjectWithSite` documents.
        return ffi::E_FAIL;
    };
    // Query outside the lock; the frame may re-enter.
    query_interface_ptr(frame, iid, out)
}

unsafe extern "system" fn visuals_set_background_color(
    this: *mut c_void,
    color: ffi::COLORREF,
) -> ffi::HRESULT {
    let base = base_from(this, offset_of!(ComObject, visuals));
    match (*base).state.lock() {
        Ok(mut state) => {
            state.set_background_color(color);
            ffi::S_OK
        }
        Err(_) => ffi::E_FAIL,
    }
}

unsafe extern "system" fn visuals_set_font(this: *mut c_void, font: ffi::HFONT) -> ffi::HRESULT {
    let base = base_from(this, offset_of!(ComObject, visuals));
    match (*base).state.lock() {
        Ok(mut state) => {
            state.set_font(font);
            ffi::S_OK
        }
        Err(_) => ffi::E_FAIL,
    }
}

unsafe extern "system" fn visuals_set_text_color(
    this: *mut c_void,
    color: ffi::COLORREF,
) -> ffi::HRESULT {
    let base = base_from(this, offset_of!(ComObject, visuals));
    match (*base).state.lock() {
        Ok(mut state) => {
            state.set_text_color(color);
            ffi::S_OK
        }
        Err(_) => ffi::E_FAIL,
    }
}

fn new_handler() -> *mut ComObject {
    Box::into_raw(Box::new(ComObject {
        unknown: IUnknownIfc {
            vtbl: &UNKNOWN_VTBL,
        },
        initialize: IInitializeWithStreamIfc {
            vtbl: &INITIALIZE_VTBL,
        },
        preview: IPreviewHandlerIfc {
            vtbl: &PREVIEW_VTBL,
        },
        visuals: IPreviewHandlerVisualsIfc {
            vtbl: &VISUALS_VTBL,
        },
        object_with_site: IObjectWithSiteIfc { vtbl: &SITE_VTBL },
        refs: AtomicU32::new(1),
        state: Mutex::new(HandlerState::default()),
        frame: Mutex::new(None),
    }))
}

// ---- class factory -------------------------------------------------------

#[repr(C)]
struct IClassFactoryVtbl {
    query_interface:
        unsafe extern "system" fn(*mut c_void, *const ffi::GUID, *mut *mut c_void) -> ffi::HRESULT,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    create_instance: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        *const ffi::GUID,
        *mut *mut c_void,
    ) -> ffi::HRESULT,
    lock_server: unsafe extern "system" fn(*mut c_void, ffi::BOOL) -> ffi::HRESULT,
}

#[repr(C)]
struct Factory {
    vtbl: *const IClassFactoryVtbl,
    refs: AtomicU32,
}

static FACTORY_VTBL: IClassFactoryVtbl = IClassFactoryVtbl {
    query_interface: factory_qi,
    add_ref: factory_add_ref,
    release: factory_release,
    create_instance: factory_create_instance,
    lock_server: factory_lock_server,
};

unsafe extern "system" fn factory_qi(
    this: *mut c_void,
    iid: *const ffi::GUID,
    out: *mut *mut c_void,
) -> ffi::HRESULT {
    if iid.is_null() || out.is_null() {
        return ffi::E_POINTER;
    }
    *out = std::ptr::null_mut();
    let iid = *iid;
    if iid == ffi::IID_IUNKNOWN || iid == ffi::IID_ICLASSFACTORY {
        let factory = this as *mut Factory;
        (*factory).refs.fetch_add(1, Ordering::Relaxed);
        *out = this;
        ffi::S_OK
    } else {
        ffi::E_NOINTERFACE
    }
}

unsafe extern "system" fn factory_add_ref(this: *mut c_void) -> u32 {
    let factory = this as *mut Factory;
    (*factory).refs.fetch_add(1, Ordering::Relaxed) + 1
}

unsafe extern "system" fn factory_release(this: *mut c_void) -> u32 {
    let factory = this as *mut Factory;
    let previous = (*factory).refs.fetch_sub(1, Ordering::AcqRel);
    if previous == 1 {
        drop(Box::from_raw(factory));
        0
    } else {
        previous - 1
    }
}

unsafe extern "system" fn factory_create_instance(
    this: *mut c_void,
    outer: *mut c_void,
    iid: *const ffi::GUID,
    out: *mut *mut c_void,
) -> ffi::HRESULT {
    let _ = this;
    if !outer.is_null() {
        return ffi::CLASS_E_NOAGGREGATION;
    }
    if iid.is_null() || out.is_null() {
        return ffi::E_POINTER;
    }
    *out = std::ptr::null_mut();
    let handler = new_handler();
    let unknown = &mut (*handler).unknown as *mut IUnknownIfc as *mut c_void;
    let table = &UNKNOWN_VTBL;
    let query = table.query_interface;
    let result = query(unknown, iid, out);
    release(handler); // drop the creation reference
    result
}

unsafe extern "system" fn factory_lock_server(
    _this: *mut c_void,
    _lock: ffi::BOOL,
) -> ffi::HRESULT {
    ffi::S_OK
}

fn new_factory() -> *mut Factory {
    Box::into_raw(Box::new(Factory {
        vtbl: &FACTORY_VTBL,
        refs: AtomicU32::new(1),
    }))
}

// ---- DLL exports ---------------------------------------------------------

fn allow_overwrite() -> bool {
    std::env::var("PARQUET_PREVIEW_ALLOW_OVERWRITE")
        .map(|value| value == "1")
        .unwrap_or(false)
}

fn register_current_module() -> ffi::HRESULT {
    let dll = match win32_registry::current_module_path() {
        Ok(dll) => dll,
        Err(_) => return ffi::E_FAIL,
    };
    match register(&Win32Registry, &dll, allow_overwrite()) {
        Ok(_) => ffi::S_OK,
        Err(_) => ffi::E_FAIL,
    }
}

fn unregister_current_module() -> ffi::HRESULT {
    match unregister(&Win32Registry) {
        Ok(_) => ffi::S_OK,
        Err(_) => ffi::E_FAIL,
    }
}

#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(
    class_id: *const ffi::GUID,
    interface_id: *const ffi::GUID,
    out: *mut *mut c_void,
) -> ffi::HRESULT {
    if class_id.is_null() || interface_id.is_null() || out.is_null() {
        return ffi::E_POINTER;
    }
    *out = std::ptr::null_mut();
    if *class_id != ffi::CLSID_PARQUET_PREVIEW {
        return ffi::CLASS_E_CLASSNOTAVAILABLE;
    }
    let factory = new_factory();
    let table = &FACTORY_VTBL;
    let query = table.query_interface;
    let result = query(factory as *mut c_void, interface_id, out);
    factory_release(factory as *mut c_void);
    result
}

#[no_mangle]
pub unsafe extern "system" fn DllCanUnloadNow() -> ffi::HRESULT {
    // The worker threads are short-lived but a preview may still be active;
    // keep the DLL loaded for the lifetime of the host process.
    ffi::S_FALSE
}

#[no_mangle]
pub extern "system" fn DllRegisterServer() -> ffi::HRESULT {
    register_current_module()
}

#[no_mangle]
pub extern "system" fn DllUnregisterServer() -> ffi::HRESULT {
    unregister_current_module()
}

#[no_mangle]
pub extern "system" fn DllMain(
    _instance: ffi::HINSTANCE,
    _reason: u32,
    _reserved: *mut c_void,
) -> ffi::BOOL {
    1
}

/// Opt-in end-to-end harness: drives the real class factory and preview handler
/// through a real `IStream` and a real isolated worker process.
///
/// This is the only test that exercises the COM, Win32 and process paths
/// together (factory -> `CreateInstance` -> QI -> `Initialize` -> `SetWindow`
/// -> `DoPreview` -> message pump -> edit text -> `Unload` -> Release), and it
/// is the safety net the L5 / M1 fixes reuse. It is skipped unless
/// `PARQUET_PREVIEW_E2E=1`, so the normal `cargo test` run is unaffected; the
/// Windows CI job runs it in a dedicated step.
///
/// Build the workspace first (`cargo build --workspace`) so
/// `parquet-preview-worker.exe` sits next to the test binary.
#[cfg(all(test, windows))]
mod e2e {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicUsize};

    use super::*;

    #[link(name = "shlwapi")]
    extern "system" {
        fn SHCreateMemStream(init: *const u8, size: u32) -> *mut c_void;
    }

    #[link(name = "user32")]
    extern "system" {
        fn GetWindow(window: ffi::HWND, command: u32) -> ffi::HWND;
        fn GetWindowTextW(window: ffi::HWND, buffer: *mut u16, max: i32) -> i32;
        fn PeekMessageW(
            msg: *mut ffi::MSG,
            window: ffi::HWND,
            min: u32,
            max: u32,
            remove: u32,
        ) -> ffi::BOOL;
        fn TranslateMessage(msg: *const ffi::MSG) -> ffi::BOOL;
        fn DispatchMessageW(msg: *const ffi::MSG) -> ffi::LRESULT;
    }

    const GW_CHILD: u32 = 5;
    const PM_REMOVE: u32 = 0x0001;

    fn opted_in() -> bool {
        std::env::var("PARQUET_PREVIEW_E2E").as_deref() == Ok("1")
    }

    /// `<target>/<profile>/parquet-preview-worker.exe`, from the test binary's
    /// own location (`<target>/<profile>/deps/`).
    fn worker_binary() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let candidate = exe.parent()?.parent()?.join("parquet-preview-worker.exe");
        candidate.exists().then_some(candidate)
    }

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    /// Every worker spool file in the temp directory.
    fn spool_files() -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
            return Vec::new();
        };
        entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.starts_with("parquet-preview-") && name.ends_with(".tmp"))
                    .unwrap_or(false)
            })
            .collect()
    }

    unsafe fn release(interface: *mut c_void) {
        release_interface(interface);
    }

    /// A minimal host `IPreviewHandlerFrame` that records the keys it is asked
    /// to translate, so the harness can prove the handler forwards them.
    struct FakeFrame {
        // Must stay first: the COM pointer the handler receives points here,
        // and the vtable is read through that pointer rather than this field.
        #[allow(dead_code)]
        vtbl: *const IPreviewHandlerFrameVtbl,
        refs: AtomicU32,
        translated: AtomicBool,
        last_key: AtomicUsize,
    }

    unsafe extern "system" fn fake_qi(
        this: *mut c_void,
        iid: *const ffi::GUID,
        out: *mut *mut c_void,
    ) -> ffi::HRESULT {
        if iid.is_null() || out.is_null() {
            return ffi::E_POINTER;
        }
        *out = std::ptr::null_mut();
        let iid = *iid;
        if iid == ffi::IID_IUNKNOWN || iid == ffi::IID_IPREVIEWHANDLERFRAME {
            let frame = this as *mut FakeFrame;
            (*frame).refs.fetch_add(1, Ordering::Relaxed);
            *out = this;
            ffi::S_OK
        } else {
            ffi::E_NOINTERFACE
        }
    }

    unsafe extern "system" fn fake_add_ref(this: *mut c_void) -> u32 {
        let frame = this as *mut FakeFrame;
        (*frame).refs.fetch_add(1, Ordering::Relaxed) + 1
    }

    unsafe extern "system" fn fake_release(this: *mut c_void) -> u32 {
        let frame = this as *mut FakeFrame;
        let previous = (*frame).refs.fetch_sub(1, Ordering::AcqRel);
        if previous == 1 {
            drop(Box::from_raw(frame));
            0
        } else {
            previous - 1
        }
    }

    unsafe extern "system" fn fake_get_window_context(
        _this: *mut c_void,
        out: *mut *mut c_void,
    ) -> ffi::HRESULT {
        if out.is_null() {
            return ffi::E_POINTER;
        }
        *out = std::ptr::null_mut();
        ffi::E_FAIL
    }

    unsafe extern "system" fn fake_translate_accelerator(
        this: *mut c_void,
        message: *mut ffi::MSG,
    ) -> ffi::HRESULT {
        let frame = this as *mut FakeFrame;
        if !message.is_null() {
            (*frame)
                .last_key
                .store((*message).wParam, Ordering::Relaxed);
            (*frame).translated.store(true, Ordering::Relaxed);
        }
        ffi::S_OK
    }

    static FAKE_FRAME_VTBL: IPreviewHandlerFrameVtbl = IPreviewHandlerFrameVtbl {
        query_interface: fake_qi,
        add_ref: fake_add_ref,
        release: fake_release,
        get_window_context: fake_get_window_context,
        translate_accelerator: fake_translate_accelerator,
    };

    fn new_fake_frame() -> *mut FakeFrame {
        Box::into_raw(Box::new(FakeFrame {
            vtbl: &FAKE_FRAME_VTBL,
            refs: AtomicU32::new(1),
            translated: AtomicBool::new(false),
            last_key: AtomicUsize::new(0),
        }))
    }

    /// A hidden top-level window to host the handler's child window.
    struct ParentWindow(ffi::HWND);

    impl ParentWindow {
        fn new() -> Self {
            let instance = win32_registry::current_module_handle().expect("module handle");
            let class = ffi::wide(&format!("ParquetPreviewE2E-{}", std::process::id()));
            let class_def = ffi::WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(ffi::DefWindowProcW),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: instance,
                hIcon: 0,
                hCursor: 0,
                hbrBackground: 0,
                lpszMenuName: std::ptr::null(),
                lpszClassName: class.as_ptr(),
            };
            unsafe { ffi::RegisterClassW(&class_def) };
            let title = ffi::wide("Parquet preview e2e");
            let hwnd = unsafe {
                ffi::CreateWindowExW(
                    0,
                    class.as_ptr(),
                    title.as_ptr(),
                    0,
                    0,
                    0,
                    400,
                    300,
                    0,
                    0,
                    instance,
                    std::ptr::null_mut(),
                )
            };
            assert_ne!(hwnd, 0, "CreateWindowExW failed for the e2e parent");
            Self(hwnd)
        }
    }

    impl Drop for ParentWindow {
        fn drop(&mut self) {
            unsafe { ffi::DestroyWindow(self.0) };
        }
    }

    /// Pump messages until the edit control has text or `timeout` elapses.
    fn pump_until_text(edit: ffi::HWND, timeout: std::time::Duration) -> String {
        let deadline = std::time::Instant::now() + timeout;
        let mut text = String::new();
        while std::time::Instant::now() < deadline {
            let mut message = ffi::MSG::default();
            // SAFETY: message is a valid MSG; the loop drains this thread's queue.
            unsafe {
                while PeekMessageW(&mut message, 0, 0, 0, PM_REMOVE) != 0 {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            let mut buffer = vec![0u16; 8192];
            let length = unsafe { GetWindowTextW(edit, buffer.as_mut_ptr(), buffer.len() as i32) };
            if length > 0 {
                text = String::from_utf16_lossy(&buffer[..length as usize]);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        text
    }

    #[test]
    fn full_preview_round_trip() {
        if !opted_in() {
            eprintln!("skipping e2e: set PARQUET_PREVIEW_E2E=1 to run");
            return;
        }
        let worker = worker_binary().expect(
            "parquet-preview-worker.exe not found; build the workspace before the e2e harness",
        );
        std::env::set_var("PARQUET_PREVIEW_WORKER", &worker);

        // COM must be initialized on this thread for DoPreview to marshal the
        // stream to the background thread (the primary M1 path). This mirrors
        // the STA Explorer runs the handler in; without it the handler falls
        // back to an inline transfer and this harness would not cover marshalling.
        let coinit =
            unsafe { ffi::CoInitializeEx(std::ptr::null_mut(), ffi::COINIT_APARTMENTTHREADED) };
        assert!(coinit >= 0, "CoInitializeEx failed (0x{coinit:08X})");

        let bytes = std::fs::read(fixture("trades.parquet")).expect("read trades.parquet");
        let parent = ParentWindow::new();
        let before_spools = spool_files();

        // Class factory straight from the DLL entry point (registration-free).
        let mut factory: *mut c_void = std::ptr::null_mut();
        let hr = unsafe {
            DllGetClassObject(
                &ffi::CLSID_PARQUET_PREVIEW,
                &ffi::IID_ICLASSFACTORY,
                &mut factory,
            )
        };
        assert_eq!(hr, ffi::S_OK, "DllGetClassObject failed (0x{hr:08X})");
        assert!(!factory.is_null(), "no class factory returned");

        // CreateInstance(IID_IPreviewHandler).
        let mut handler: *mut c_void = std::ptr::null_mut();
        let hr = unsafe {
            let table = *(factory as *const *const IClassFactoryVtbl);
            ((*table).create_instance)(
                factory,
                std::ptr::null_mut(),
                &ffi::IID_IPREVIEWHANDLER,
                &mut handler,
            )
        };
        assert_eq!(
            hr,
            ffi::S_OK,
            "IClassFactory::CreateInstance failed (0x{hr:08X})"
        );

        // QI for IInitializeWithStream.
        let mut initialize: *mut c_void = std::ptr::null_mut();
        let hr = unsafe {
            let table = *(handler as *const *const IPreviewHandlerVtbl);
            ((*table).query_interface)(handler, &ffi::IID_IINITIALIZEWITHSTREAM, &mut initialize)
        };
        assert_eq!(
            hr,
            ffi::S_OK,
            "QI IInitializeWithStream failed (0x{hr:08X})"
        );

        // Attach a fake host frame through IObjectWithSite (L5 sub-step 1).
        let frame = new_fake_frame();
        let mut site: *mut c_void = std::ptr::null_mut();
        let hr = unsafe {
            let table = *(handler as *const *const IPreviewHandlerVtbl);
            ((*table).query_interface)(handler, &ffi::IID_IOBJECTWITHSITE, &mut site)
        };
        assert_eq!(hr, ffi::S_OK, "QI IObjectWithSite failed (0x{hr:08X})");
        let hr = unsafe {
            let table = *(site as *const *const IObjectWithSiteVtbl);
            ((*table).set_site)(site, frame as *mut c_void)
        };
        assert_eq!(hr, ffi::S_OK, "IObjectWithSite::SetSite failed");

        // A real IStream over the fixture bytes (STGM_READ == 0).
        let stream = unsafe { SHCreateMemStream(bytes.as_ptr(), bytes.len() as u32) };
        assert!(!stream.is_null(), "SHCreateMemStream failed");
        let hr = unsafe {
            let table = *(initialize as *const *const IInitializeWithStreamVtbl);
            ((*table).initialize)(initialize, stream, 0)
        };
        assert_eq!(
            hr,
            ffi::S_OK,
            "IInitializeWithStream::Initialize failed (0x{hr:08X})"
        );

        // SetWindow + DoPreview.
        let rect = ffi::RECT {
            left: 0,
            top: 0,
            right: 400,
            bottom: 300,
        };
        unsafe {
            let table = *(handler as *const *const IPreviewHandlerVtbl);
            assert_eq!(
                ((*table).set_window)(handler, parent.0, &rect),
                ffi::S_OK,
                "SetWindow failed"
            );
            assert_eq!(
                ((*table).do_preview)(handler),
                ffi::S_OK,
                "DoPreview failed"
            );
        }

        // The handler's window is the parent's only child; the edit is its child.
        let pane = unsafe { GetWindow(parent.0, GW_CHILD) };
        assert_ne!(pane, 0, "the handler did not create its window");
        let edit = unsafe { GetWindow(pane, GW_CHILD) };
        assert_ne!(edit, 0, "the handler did not create the edit control");

        let text = pump_until_text(edit, std::time::Duration::from_secs(30));
        assert!(
            !text.is_empty(),
            "no preview text appeared before the deadline"
        );
        assert!(
            text.contains("Rows: 60"),
            "the preview did not report the fixture's 60 rows: {:.200}",
            text
        );
        // The text layer emits LF; the handler converts to CRLF for the EDIT
        // control (H1). This is only observable through a real control.
        assert!(
            text.contains("\r\n"),
            "the edit control received LF-only text: {:.200}",
            text
        );
        assert!(
            !text.contains("Could not preview"),
            "the handler fell back to an error: {:.200}",
            text
        );

        // L5 sub-step 1: an unhandled key (Tab) must be forwarded to the host
        // frame, which the fake records. VK_TAB == 0x09.
        let tab = ffi::MSG {
            message: ffi::WM_KEYDOWN,
            wParam: 0x09,
            ..ffi::MSG::default()
        };
        let hr = unsafe {
            let table = *(handler as *const *const IPreviewHandlerVtbl);
            ((*table).translate_accelerator)(handler, &tab)
        };
        assert_eq!(hr, ffi::S_OK, "the frame did not accept Tab (0x{hr:08X})");
        assert!(
            unsafe { (*frame).translated.load(Ordering::Relaxed) },
            "the handler did not forward Tab to the host frame"
        );
        assert_eq!(
            unsafe { (*frame).last_key.load(Ordering::Relaxed) },
            0x09,
            "the frame was given the wrong key"
        );

        // L5 sub-step 2: SetFont must reach the edit control (WM_GETFONT).
        let mut visuals: *mut c_void = std::ptr::null_mut();
        let hr = unsafe {
            let table = *(handler as *const *const IPreviewHandlerVtbl);
            ((*table).query_interface)(handler, &ffi::IID_IPREVIEWHANDLERVISUALS, &mut visuals)
        };
        assert_eq!(
            hr,
            ffi::S_OK,
            "QI IPreviewHandlerVisuals failed (0x{hr:08X})"
        );
        let requested = unsafe { ffi::GetStockObject(ffi::ANSI_FIXED_FONT) };
        assert_ne!(requested, 0, "GetStockObject(ANSI_FIXED_FONT) failed");
        let hr = unsafe {
            let table = *(visuals as *const *const IPreviewHandlerVisualsVtbl);
            ((*table).set_font)(visuals, requested)
        };
        assert_eq!(hr, ffi::S_OK, "IPreviewHandlerVisuals::SetFont failed");
        let actual = unsafe { ffi::SendMessageW(edit, ffi::WM_GETFONT, 0, 0) };
        assert_eq!(
            actual, requested,
            "the edit control did not receive the requested font"
        );

        // Unload terminates and joins the worker, then destroys the windows.
        unsafe {
            let table = *(handler as *const *const IPreviewHandlerVtbl);
            assert_eq!(((*table).unload)(handler), ffi::S_OK, "Unload failed");
        }

        // The worker must be gone: no spool file may outlive it.
        let mut left = spool_files();
        for _ in 0..40 {
            left.retain(|path| !before_spools.contains(path));
            if left.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            left = spool_files();
        }
        left.retain(|path| !before_spools.contains(path));
        assert!(left.is_empty(), "spool files survived Unload: {left:?}");

        // Release every reference: visuals, site, initialize, handler, factory,
        // then the stream (the handler already released its own stream reference
        // in Unload). The handler's frame reference went with the handler; the
        // test's own frame reference is dropped last.
        unsafe {
            release(visuals);
            release(site);
            release(initialize);
            release(handler);
            release(factory);
            crate::stream::release(stream);
            release(frame as *mut c_void);
            ffi::CoUninitialize();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clsid_matches_registration_constant() {
        assert_eq!(
            ffi::CLSID_PARQUET_PREVIEW.to_registry_string(),
            crate::registration::CLSID
        );
    }

    #[test]
    fn interface_guids_have_expected_text() {
        assert_eq!(
            ffi::IID_IPREVIEWHANDLER.to_registry_string(),
            crate::registration::PREVIEW_HANDLER_SHELLEXT
        );
        assert_eq!(
            ffi::IID_IINITIALIZEWITHSTREAM.to_registry_string(),
            "{B824B49D-22AC-4161-AC8A-9916E8FA3F7F}"
        );
    }
}

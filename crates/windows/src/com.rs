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
    translate_accelerator:
        unsafe extern "system" fn(*mut c_void, *const ffi::MSG) -> ffi::HRESULT,
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
struct ComObject {
    // Each interface starts with its own vtable pointer. `unknown` must stay
    // first so an IUnknown pointer is also the allocation base.
    unknown: IUnknownIfc,
    initialize: IInitializeWithStreamIfc,
    preview: IPreviewHandlerIfc,
    visuals: IPreviewHandlerVisualsIfc,
    refs: AtomicU32,
    state: Mutex<HandlerState>,
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
    set_window: set_window,
    set_rect: set_rect,
    do_preview: do_preview,
    unload: unload,
    set_focus: set_focus,
    query_focus: query_focus,
    translate_accelerator: translate_accelerator,
};

static VISUALS_VTBL: IPreviewHandlerVisualsVtbl = IPreviewHandlerVisualsVtbl {
    query_interface: visuals_qi,
    add_ref: visuals_add_ref,
    release: visuals_release,
    set_background_color: visuals_set_background_color,
    set_font: visuals_set_font,
    set_text_color: visuals_set_text_color,
};

unsafe fn base_from(this: *mut c_void, offset: usize) -> *mut ComObject {
    (this as *mut u8).sub(offset) as *mut ComObject
}

unsafe fn add_ref(base: *mut ComObject) -> u32 {
    (*base).refs.fetch_add(1, Ordering::Relaxed) + 1
}

unsafe fn release(base: *mut ComObject) -> u32 {
    let previous = (*base).refs.fetch_sub(1, Ordering::AcqRel);
    if previous == 1 {
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
            query_interface(
                base_from(this, offset_of!(ComObject, $field)),
                iid,
                out,
            )
        }

        unsafe extern "system" fn $add(this: *mut c_void) -> u32 {
            add_ref(base_from(this, offset_of!(ComObject, $field)))
        }

        unsafe extern "system" fn $rel(this: *mut c_void) -> u32 {
            release(base_from(this, offset_of!(ComObject, $field)))
        }
    };
}

refcount_bridge!(unknown_qi, unknown_add_ref, unknown_release, IUnknownIfc, unknown);
refcount_bridge!(
    initialize_qi,
    initialize_add_ref,
    initialize_release,
    IInitializeWithStreamIfc,
    initialize
);
refcount_bridge!(preview_qi, preview_add_ref, preview_release, IPreviewHandlerIfc, preview);
refcount_bridge!(
    visuals_qi,
    visuals_add_ref,
    visuals_release,
    IPreviewHandlerVisualsIfc,
    visuals
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

unsafe extern "system" fn set_rect(
    this: *mut c_void,
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

unsafe extern "system" fn query_focus(
    this: *mut c_void,
    focus: *mut ffi::HWND,
) -> ffi::HRESULT {
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
    if let Ok(state) = (*base).state.lock() {
        state.translate_accelerator(&*message)
    } else {
        ffi::E_FAIL
    }
}

unsafe extern "system" fn visuals_set_background_color(
    _this: *mut c_void,
    _color: ffi::COLORREF,
) -> ffi::HRESULT {
    ffi::S_OK
}

unsafe extern "system" fn visuals_set_font(
    _this: *mut c_void,
    _font: ffi::HFONT,
) -> ffi::HRESULT {
    ffi::S_OK
}

unsafe extern "system" fn visuals_set_text_color(
    _this: *mut c_void,
    _color: ffi::COLORREF,
) -> ffi::HRESULT {
    ffi::S_OK
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
        refs: AtomicU32::new(1),
        state: Mutex::new(HandlerState::default()),
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

// SPDX-License-Identifier: Apache-2.0
//! Minimal Win32/COM FFI surface.
//!
//! Only the functions the v1 handler needs are declared. The module is compiled
//! on Windows and can be type-checked on Linux with the `win-check` feature; it
//! does not link there.

#![allow(non_snake_case, non_camel_case_types)]

use std::ffi::c_void;

pub type HRESULT = i32;
pub type BOOL = i32;
pub type ATOM = u16;
pub type WPARAM = usize;
pub type LPARAM = isize;
pub type LRESULT = isize;
pub type HWND = isize;
pub type HINSTANCE = isize;
pub type HMODULE = isize;
pub type HICON = isize;
pub type HCURSOR = isize;
pub type HBRUSH = isize;
pub type HMENU = isize;
pub type HFONT = isize;
pub type HKEY = isize;
pub type HANDLE = isize;
pub type COLORREF = u32;
pub type LSTATUS = i32;

pub const S_OK: HRESULT = 0;
pub const S_FALSE: HRESULT = 1;
pub const E_NOINTERFACE: HRESULT = 0x8000_4002u32 as HRESULT;
pub const E_POINTER: HRESULT = 0x8000_4003u32 as HRESULT;
pub const E_INVALIDARG: HRESULT = 0x8007_0057u32 as HRESULT;
pub const E_FAIL: HRESULT = 0x8000_4005u32 as HRESULT;
pub const CLASS_E_NOAGGREGATION: HRESULT = 0x8004_0110u32 as HRESULT;
pub const CLASS_E_CLASSNOTAVAILABLE: HRESULT = 0x8004_0111u32 as HRESULT;

// Predefined registry handles are sign-extended pseudo-handles on 64-bit.
pub const HKEY_CURRENT_USER: HKEY = 0x8000_0001u32 as i32 as isize;
pub const REG_SZ: u32 = 1;
pub const RRF_RT_REG_SZ: u32 = 0x0000_0002;
pub const ERROR_SUCCESS: LSTATUS = 0;
pub const ERROR_FILE_NOT_FOUND: LSTATUS = 2;
pub const ERROR_MORE_DATA: LSTATUS = 234;

pub const STGM_WRITE: u32 = 0x0000_0001;
pub const STGM_READWRITE: u32 = 0x0000_0002;
pub const STREAM_SEEK_SET: u32 = 0;
pub const STATFLAG_NONAME: u32 = 1;

pub const WM_SIZE: u32 = 0x0005;
pub const WM_ERASEBKGND: u32 = 0x0014;
pub const WM_DESTROY: u32 = 0x0002;
pub const WM_KEYDOWN: u32 = 0x0100;
pub const VK_F1: usize = 0x70;

pub const MB_OK: u32 = 0x0000_0000;
pub const MB_ICONINFORMATION: u32 = 0x0000_0040;

pub const WS_CHILD: u32 = 0x4000_0000;
pub const WS_VISIBLE: u32 = 0x1000_0000;
pub const WS_CLIPCHILDREN: u32 = 0x0200_0000;
pub const WS_VSCROLL: u32 = 0x0020_0000;
pub const WS_TABSTOP: u32 = 0x0001_0000;
pub const ES_MULTILINE: u32 = 0x0004;
pub const ES_AUTOVSCROLL: u32 = 0x0040;
pub const ES_READONLY: u32 = 0x0800;
pub const SWP_NOZORDER: u32 = 0x0004;
pub const SWP_NOACTIVATE: u32 = 0x0010;
pub const GWLP_WNDPROC: i32 = -4;
pub const GWLP_USERDATA: i32 = -21;

pub const JOB_OBJECT_LIMIT_PROCESS_MEMORY: u32 = 0x0000_0100;
pub const JOB_OBJECT_LIMIT_ACTIVE_PROCESS: u32 = 0x0000_0008;
pub const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;
pub const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: i32 = 9;
pub const PROCESS_SET_QUOTA: u32 = 0x0100;
pub const PROCESS_TERMINATE: u32 = 0x0001;
pub const GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS: u32 = 0x0000_0004;
pub const GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT: u32 = 0x0000_0002;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

impl GUID {
    pub const fn from_u128(value: u128) -> Self {
        Self {
            data1: (value >> 96) as u32,
            data2: (value >> 80) as u16,
            data3: (value >> 64) as u16,
            data4: [
                (value >> 56) as u8,
                (value >> 48) as u8,
                (value >> 40) as u8,
                (value >> 32) as u8,
                (value >> 24) as u8,
                (value >> 16) as u8,
                (value >> 8) as u8,
                value as u8,
            ],
        }
    }

    #[allow(dead_code)]
    pub fn to_registry_string(self) -> String {
        format!(
            "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
            self.data1,
            self.data2,
            self.data3,
            self.data4[0],
            self.data4[1],
            self.data4[2],
            self.data4[3],
            self.data4[4],
            self.data4[5],
            self.data4[6],
            self.data4[7]
        )
    }
}

pub const IID_IUNKNOWN: GUID = GUID::from_u128(0x00000000_0000_0000_c000_000000000046);
pub const IID_ICLASSFACTORY: GUID = GUID::from_u128(0x00000001_0000_0000_c000_000000000046);
pub const IID_IINITIALIZEWITHSTREAM: GUID = GUID::from_u128(0xb824b49d_22ac_4161_ac8a_9916e8fa3f7f);
pub const IID_IPREVIEWHANDLER: GUID = GUID::from_u128(0x8895b1c6_b41f_4c1c_a562_0d564250836f);
pub const IID_IPREVIEWHANDLERVISUALS: GUID =
    GUID::from_u128(0x196bf9a5_b346_4ef0_aa1e_5dcdb76768b1);
pub const CLSID_PARQUET_PREVIEW: GUID = GUID::from_u128(0x7f2c9e4a_2b6d_4b7e_9c3a_5d1e8a0f6b21);

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct RECT {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct POINT {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct MSG {
    pub hwnd: HWND,
    pub message: u32,
    pub wParam: WPARAM,
    pub lParam: LPARAM,
    pub time: u32,
    pub pt: POINT,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct FILETIME {
    pub dwLowDateTime: u32,
    pub dwHighDateTime: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct STATSTG {
    pub pwcsName: *mut u16,
    pub type_: u32,
    pub cbSize: u64,
    pub mtime: FILETIME,
    pub ctime: FILETIME,
    pub atime: FILETIME,
    pub grfMode: u32,
    pub grfLocksSupported: u32,
    pub clsid: GUID,
    pub grfStateBits: u32,
    pub reserved: u32,
}

pub type WNDPROC = Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>;

#[repr(C)]
pub struct WNDCLASSW {
    pub style: u32,
    pub lpfnWndProc: WNDPROC,
    pub cbClsExtra: i32,
    pub cbWndExtra: i32,
    pub hInstance: HINSTANCE,
    pub hIcon: HICON,
    pub hCursor: HCURSOR,
    pub hbrBackground: HBRUSH,
    pub lpszMenuName: *const u16,
    pub lpszClassName: *const u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct JOBOBJECT_BASIC_LIMIT_INFORMATION {
    pub PerProcessUserTimeLimit: i64,
    pub PerJobUserTimeLimit: i64,
    pub LimitFlags: u32,
    pub MinimumWorkingSetSize: usize,
    pub MaximumWorkingSetSize: usize,
    pub ActiveProcessLimit: u32,
    pub Affinity: usize,
    pub PriorityClass: u32,
    pub SchedulingClass: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct IO_COUNTERS {
    pub ReadOperationCount: u64,
    pub WriteOperationCount: u64,
    pub OtherOperationCount: u64,
    pub ReadTransferCount: u64,
    pub WriteTransferCount: u64,
    pub OtherTransferCount: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
    pub BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION,
    pub IoInfo: IO_COUNTERS,
    pub ProcessMemoryLimit: usize,
    pub JobMemoryLimit: usize,
    pub PeakProcessMemoryUsed: usize,
    pub PeakJobMemoryUsed: usize,
}

#[link(name = "kernel32")]
extern "system" {
    pub fn GetModuleHandleExW(flags: u32, name: *const u16, module: *mut HMODULE) -> BOOL;
    pub fn GetModuleFileNameW(module: HMODULE, buffer: *mut u16, size: u32) -> u32;
    pub fn OpenProcess(access: u32, inherit: BOOL, pid: u32) -> HANDLE;
    pub fn TerminateProcess(process: HANDLE, code: u32) -> BOOL;
    pub fn CloseHandle(handle: HANDLE) -> BOOL;
    pub fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> HANDLE;
    pub fn SetInformationJobObject(
        job: HANDLE,
        class: i32,
        info: *const c_void,
        length: u32,
    ) -> BOOL;
    pub fn AssignProcessToJobObject(job: HANDLE, process: HANDLE) -> BOOL;
    pub fn TerminateJobObject(job: HANDLE, code: u32) -> BOOL;
}

#[link(name = "user32")]
extern "system" {
    pub fn RegisterClassW(class: *const WNDCLASSW) -> ATOM;
    pub fn CreateWindowExW(
        ex_style: u32,
        class_name: *const u16,
        window_name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: HWND,
        menu: HMENU,
        instance: HINSTANCE,
        param: *mut c_void,
    ) -> HWND;
    pub fn DestroyWindow(window: HWND) -> BOOL;
    pub fn DefWindowProcW(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT;
    pub fn SetWindowPos(
        window: HWND,
        after: HWND,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        flags: u32,
    ) -> BOOL;
    pub fn SetWindowTextW(window: HWND, text: *const u16) -> BOOL;
    pub fn PostMessageW(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> BOOL;
    pub fn GetClientRect(window: HWND, rect: *mut RECT) -> BOOL;
    pub fn MessageBoxW(window: HWND, text: *const u16, caption: *const u16, flags: u32) -> i32;
    pub fn SetFocus(window: HWND) -> HWND;
    pub fn GetFocus() -> HWND;
    pub fn SetWindowLongPtrW(window: HWND, index: i32, value: isize) -> isize;
    pub fn GetWindowLongPtrW(window: HWND, index: i32) -> isize;
    pub fn CallWindowProcW(
        previous: WNDPROC,
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT;
}

#[link(name = "advapi32")]
extern "system" {
    pub fn RegGetValueW(
        key: HKEY,
        subkey: *const u16,
        value: *const u16,
        flags: u32,
        kind: *mut u32,
        data: *mut c_void,
        size: *mut u32,
    ) -> LSTATUS;
    pub fn RegSetKeyValueW(
        key: HKEY,
        subkey: *const u16,
        value: *const u16,
        kind: u32,
        data: *const c_void,
        size: u32,
    ) -> LSTATUS;
    pub fn RegDeleteTreeW(key: HKEY, subkey: *const u16) -> LSTATUS;
    pub fn RegDeleteKeyValueW(key: HKEY, subkey: *const u16, value: *const u16) -> LSTATUS;
    pub fn RegOpenKeyExW(
        key: HKEY,
        subkey: *const u16,
        options: u32,
        access: u32,
        result: *mut HKEY,
    ) -> LSTATUS;
    pub fn RegCloseKey(key: HKEY) -> LSTATUS;
    pub fn RegQueryInfoKeyW(
        key: HKEY,
        class: *mut u16,
        class_len: *mut u32,
        reserved: *mut u32,
        subkeys: *mut u32,
        max_subkey: *mut u32,
        max_class: *mut u32,
        values: *mut u32,
        max_value_name: *mut u32,
        max_value_len: *mut u32,
        security: *mut u32,
        last_write: *mut c_void,
    ) -> LSTATUS;
    pub fn RegDeleteKeyW(key: HKEY, subkey: *const u16) -> LSTATUS;
}

/// NUL-terminated UTF-16 conversion for Win32 calls.
pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

// SPDX-License-Identifier: Apache-2.0
//! Preview pane window and worker orchestration.
//!
//! `DoPreview` marshals the host stream for a background thread (no RAM
//! snapshot); that thread streams it to the isolated worker in bounded chunks
//! and posts the rendered text into a read-only edit control. Replacing the
//! selection kills the worker job and signals the transfer to stop.

use std::ffi::c_void;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;

use parquet_preview_client::{CancelReader, WorkerClient};
use parquet_preview_core::about::about_text;
use parquet_preview_core::PreviewOptions;

use crate::ffi;
use crate::job::JobGuard;
use crate::stream::{self, StreamReader};
use crate::win32_registry;

const WM_APP_PREVIEW_READY: u32 = 0x8000 + 1;

struct WindowContext {
    edit: ffi::HWND,
    text: Arc<Mutex<Option<String>>>,
    colors: Arc<Mutex<Colors>>,
}

/// `IPreviewHandlerVisuals` colours, applied through `WM_CTLCOLORSTATIC`.
#[derive(Clone, Copy)]
struct Colors {
    background: ffi::COLORREF,
    text_color: ffi::COLORREF,
}

impl Default for Colors {
    fn default() -> Self {
        // Light system colours; Explorer overrides them for dark mode.
        Self {
            background: 0x00FF_FFFF,
            text_color: 0x0000_0000,
        }
    }
}

pub struct HandlerState {
    stream: Option<*mut c_void>,
    hwnd: ffi::HWND,
    edit: ffi::HWND,
    thread: Option<JoinHandle<()>>,
    cancel: Arc<AtomicBool>,
    job: Arc<Mutex<Option<JobGuard>>>,
    text: Arc<Mutex<Option<String>>>,
    font: ffi::HFONT,
    colors: Arc<Mutex<Colors>>,
}

// The state is only touched from the COM apartment thread. The worker thread
// receives only copies of the window handles and owned stream bytes.
unsafe impl Send for HandlerState {}

impl Default for HandlerState {
    fn default() -> Self {
        Self {
            stream: None,
            hwnd: 0,
            edit: 0,
            thread: None,
            cancel: Arc::new(AtomicBool::new(false)),
            job: Arc::new(Mutex::new(None)),
            text: Arc::new(Mutex::new(None)),
            font: unsafe { ffi::GetStockObject(ffi::DEFAULT_GUI_FONT) },
            colors: Arc::new(Mutex::new(Colors::default())),
        }
    }
}

impl HandlerState {
    /// Take a reference to the supplied COM stream. Returns an HRESULT.
    pub unsafe fn initialize(&mut self, stream: *mut c_void, mode: u32) -> ffi::HRESULT {
        if stream.is_null() {
            return ffi::E_POINTER;
        }
        if mode & (ffi::STGM_WRITE | ffi::STGM_READWRITE) != 0 {
            return ffi::E_INVALIDARG;
        }
        self.release_stream();
        stream::add_ref(stream);
        self.stream = Some(stream);
        ffi::S_OK
    }

    pub fn set_window(&mut self, parent: ffi::HWND, rect: &ffi::RECT) -> Result<(), String> {
        if parent == 0 {
            return Err("preview parent window is null".to_string());
        }
        if self.hwnd != 0 && self.hwnd != parent {
            self.destroy_window();
        }
        if self.hwnd == 0 {
            let (hwnd, edit) = create_window(parent, rect, self.text.clone(), self.colors.clone())?;
            self.hwnd = hwnd;
            self.edit = edit;
            apply_font(self.edit, self.font);
        }
        self.set_rect(rect);
        Ok(())
    }

    pub fn set_rect(&mut self, rect: &ffi::RECT) {
        if self.hwnd == 0 {
            return;
        }
        let width = (rect.right - rect.left).max(0);
        let height = (rect.bottom - rect.top).max(0);
        unsafe {
            ffi::SetWindowPos(
                self.hwnd,
                0,
                rect.left,
                rect.top,
                width,
                height,
                ffi::SWP_NOZORDER | ffi::SWP_NOACTIVATE,
            );
        }
        resize_edit(self.hwnd, self.edit);
    }

    pub fn do_preview(&mut self) {
        self.stop_preview();
        let Some(stream) = self.stream else {
            self.set_text(&error_text("the host supplied no stream"));
            return;
        };
        if self.hwnd == 0 || self.edit == 0 {
            return;
        }
        if let Err(error) = unsafe { stream::seek_to_start(stream) } {
            self.set_text(&error_text(&error));
            return;
        }
        let size = match unsafe { stream::stream_size(stream) } {
            Ok(size) if size > 0 => size,
            Ok(_) => {
                self.set_text(&error_text("the supplied stream is empty"));
                return;
            }
            Err(error) => {
                self.set_text(&error_text(&error));
                return;
            }
        };
        // Marshal the stream so a background thread can read it without copying
        // the file into RAM. Standard shell streams marshal; a stream that does
        // not is read here in bounded chunks instead.
        match unsafe { stream::marshal(stream) } {
            Ok(marshalled) => self.start_marshalled_preview(marshalled, size),
            Err(_) => self.start_inline_preview(stream, size),
        }
    }

    /// Primary path: the marshalled stream is unmarshalled on a background
    /// thread, which streams it to the worker in bounded chunks.
    fn start_marshalled_preview(&mut self, marshalled: *mut c_void, size: u64) {
        let cancelled = Arc::new(AtomicBool::new(false));
        let job = Arc::new(Mutex::new(None));
        let parent = self.hwnd;
        let text_slot = self.text.clone();
        if let Ok(mut current) = text_slot.lock() {
            *current = None;
        }
        let cancel = cancelled.clone();
        let job_slot = job.clone();
        let pointer = SendPtr(marshalled);
        self.cancel = cancelled;
        self.job = job;
        self.thread = Some(std::thread::spawn(move || {
            let text = match run_preview(pointer, size, &cancel, &job_slot) {
                Ok(text) => text,
                Err(error) => error_text(&error),
            };
            if !cancel.load(Ordering::Relaxed) {
                if let Ok(mut slot) = text_slot.lock() {
                    *slot = Some(text);
                }
                unsafe {
                    ffi::PostMessageW(parent, WM_APP_PREVIEW_READY, 0, 0);
                }
            }
            if let Ok(mut job) = job_slot.lock() {
                job.take();
            }
        }));
    }

    /// Fallback for a stream that cannot be marshalled: stream it from this
    /// thread in bounded chunks. Memory stays bounded (the snapshot and its
    /// 64 MiB ceiling are gone either way); only the host apartment is occupied
    /// for the duration, as it was before this fix.
    fn start_inline_preview(&mut self, stream: *mut c_void, size: u64) {
        let worker = match worker_path() {
            Ok(worker) => worker,
            Err(error) => {
                self.set_text(&error_text(&error));
                return;
            }
        };
        let client = match WorkerClient::spawn(&worker, "explorer-preview") {
            Ok(client) => client,
            Err(error) => {
                self.set_text(&error_text(&format!(
                    "cannot start the isolated worker: {error}"
                )));
                return;
            }
        };
        match JobGuard::create_for_pid(client.pid()) {
            Ok(job) => {
                if let Ok(mut slot) = self.job.lock() {
                    *slot = Some(job);
                }
            }
            Err(error) => {
                self.set_text(&error_text(&format!("cannot apply worker limits: {error}")));
                return;
            }
        }
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let mut reader = unsafe { StreamReader::new(stream) };
        let text = match pump_and_render(client, &mut reader, size, &cancel) {
            Ok(text) => text,
            Err(error) => error_text(&error),
        };
        self.set_text(&text);
    }

    pub fn unload(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        // The job object (kill-on-close) owns termination of the worker; there
        // is no separate PID kill path to race with PID recycling.
        if let Ok(mut job) = self.job.lock() {
            if let Some(job) = job.take() {
                job.terminate();
            }
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        self.release_stream();
        self.destroy_window();
    }

    pub fn set_focus(&self) {
        if self.edit != 0 {
            unsafe {
                ffi::SetFocus(self.edit);
            }
        }
    }

    /// Apply Explorer's UI font (`IPreviewHandlerVisuals::SetFont`).
    pub fn set_font(&mut self, font: ffi::HFONT) {
        self.font = font;
        apply_font(self.edit, font);
    }

    /// Apply Explorer's background colour (`SetBackgroundColor`).
    pub fn set_background_color(&mut self, color: ffi::COLORREF) {
        if let Ok(mut colors) = self.colors.lock() {
            colors.background = color;
        }
        self.repaint();
    }

    /// Apply Explorer's text colour (`SetTextColor`).
    pub fn set_text_color(&mut self, color: ffi::COLORREF) {
        if let Ok(mut colors) = self.colors.lock() {
            colors.text_color = color;
        }
        self.repaint();
    }

    fn repaint(&self) {
        if self.edit != 0 {
            unsafe {
                ffi::InvalidateRect(self.edit, std::ptr::null(), 1);
            }
        }
    }

    pub fn query_focus(&self) -> ffi::HWND {
        unsafe { ffi::GetFocus() }
    }

    pub fn translate_accelerator(&self, message: &ffi::MSG) -> ffi::HRESULT {
        if message.message == ffi::WM_KEYDOWN && message.wParam == ffi::VK_F1 {
            show_about(self.hwnd);
            return ffi::S_OK;
        }
        ffi::S_FALSE
    }

    fn stop_preview(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        // Terminating the job kills the worker; the PID is never used to kill
        // (see L3), so there is nothing here that can target a recycled PID.
        if let Ok(mut job) = self.job.lock() {
            if let Some(job) = job.take() {
                job.terminate();
            }
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }

    fn set_text(&self, text: &str) {
        if self.edit != 0 {
            set_window_text(self.edit, text);
        }
    }

    fn release_stream(&mut self) {
        if let Some(stream) = self.stream.take() {
            unsafe {
                stream::release(stream);
            }
        }
    }

    fn destroy_window(&mut self) {
        if self.hwnd != 0 {
            unsafe {
                ffi::DestroyWindow(self.hwnd);
            }
            self.hwnd = 0;
            self.edit = 0;
        }
    }
}

impl Drop for HandlerState {
    fn drop(&mut self) {
        self.unload();
    }
}

/// A raw interface pointer moved to a background thread.
///
/// Safe by the same argument as `unsafe impl Send for HandlerState`: the pointer
/// names a COM object that was explicitly marshalled for the destination thread,
/// and the thread is the sole owner of the reference.
struct SendPtr(*mut c_void);

unsafe impl Send for SendPtr {}

/// Background-thread half of the marshalled preview path.
fn run_preview(
    marshalled: SendPtr,
    size: u64,
    cancel: &AtomicBool,
    job_slot: &Mutex<Option<JobGuard>>,
) -> Result<String, String> {
    // Unmarshalling also initializes COM for this thread; the returned value
    // releases the stream and tears the apartment down when it drops (after the
    // worker client, so the worker is gone first).
    let stream = unsafe { stream::unmarshal(marshalled.0) }?;
    unsafe { stream::seek_to_start(stream.as_ptr()) }?;
    let worker = worker_path()?;
    if cancel.load(Ordering::Relaxed) {
        return Err("preview was cancelled".to_string());
    }
    let mut client = WorkerClient::spawn(&worker, "explorer-preview")
        .map_err(|e| format!("cannot start the isolated worker: {e}"))?;
    let job = JobGuard::create_for_pid(client.pid())
        .map_err(|e| format!("cannot apply worker limits: {e}"))?;
    if let Ok(mut slot) = job_slot.lock() {
        *slot = Some(job);
    }
    if cancel.load(Ordering::Relaxed) {
        client.kill();
        return Err("preview was cancelled".to_string());
    }
    let mut reader = unsafe { StreamReader::new(stream.as_ptr()) };
    pump_and_render(client, &mut reader, size, cancel)
}

/// Stream `reader` into the worker in bounded chunks and render it. The
/// transfer stops between chunks as soon as `cancel` is set, so replacing the
/// selection interrupts a slow read without changing the protocol.
fn pump_and_render(
    mut client: WorkerClient,
    reader: &mut dyn Read,
    size: u64,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let mut reader = CancelReader::new(reader, cancel);
    client
        .open_stream(&mut reader, size)
        .map_err(|e| format!("cannot transfer the stream: {e}"))?;
    client
        .inspect()
        .map_err(|e| format!("cannot inspect this Parquet file: {e}"))?;
    let options = PreviewOptions {
        display_name: None,
        sample_offset: 0,
        sample_limit: 100,
        scan: false,
        market_profile: None,
        baseline: None,
        diagnostic_rows: 50_000,
    };
    let (_, text) = client
        .render(options)
        .map_err(|e| format!("cannot render the preview: {e}"))?;
    let _ = client.shutdown();
    Ok(text)
}

fn worker_path() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("PARQUET_PREVIEW_WORKER") {
        if !path.trim().is_empty() {
            return Ok(PathBuf::from(path));
        }
    }
    let dll = win32_registry::current_module_path()?;
    let directory = PathBuf::from(dll)
        .parent()
        .map(PathBuf::from)
        .ok_or_else(|| "cannot locate the handler directory".to_string())?;
    Ok(directory.join("parquet-preview-worker.exe"))
}

fn error_text(message: &str) -> String {
    format!(
        "Parquet preview\nMade by Maria\n\nCould not preview this file.\n\n{message}\n\n\
         Read-only. No file was modified or uploaded. Not affiliated with MetaQuotes; \
         MetaTrader 5 and MT5 are trademarks of MetaQuotes Ltd."
    )
}

fn show_about(parent: ffi::HWND) {
    let text = ffi::wide(&about_text());
    let caption = ffi::wide("About Parquet preview");
    unsafe {
        ffi::MessageBoxW(
            parent,
            text.as_ptr(),
            caption.as_ptr(),
            ffi::MB_OK | ffi::MB_ICONINFORMATION,
        );
    }
}

/// Convert LF-only text to CRLF for a Win32 multiline `EDIT` control, which
/// only breaks lines on CRLF (its lone-LF handling needs `ES_EX_ALLOWEOL_LF`,
/// Windows 10 1809+). The text layer (core `report`, `error_text`) emits bare
/// `\n`, so without this the whole preview renders as one wrapped paragraph.
/// Idempotent: an existing `\r\n` is left alone and never becomes `\r\r\n`.
fn to_crlf(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut previous_was_cr = false;
    for ch in text.chars() {
        if ch == '\n' && !previous_was_cr {
            out.push('\r');
        }
        out.push(ch);
        previous_was_cr = ch == '\r';
    }
    out
}

fn set_window_text(window: ffi::HWND, text: &str) {
    let text = ffi::wide(&to_crlf(text));
    unsafe {
        ffi::SetWindowTextW(window, text.as_ptr());
    }
}

fn window_class_name() -> &'static [u16] {
    static NAME: OnceLock<Vec<u16>> = OnceLock::new();
    NAME.get_or_init(|| ffi::wide(&format!("ParquetPreviewPane-{}", std::process::id())))
}

fn create_window(
    parent: ffi::HWND,
    rect: &ffi::RECT,
    text: Arc<Mutex<Option<String>>>,
    colors: Arc<Mutex<Colors>>,
) -> Result<(ffi::HWND, ffi::HWND), String> {
    static CLASS: OnceLock<Result<(), String>> = OnceLock::new();
    CLASS.get_or_init(register_class).clone()?;
    let class = window_class_name();
    let edit_class = ffi::wide("EDIT");
    let title = ffi::wide("Parquet preview");
    let instance = win32_registry::current_module_handle().map_err(|e| e.to_string())?;
    if instance == 0 {
        return Err("GetModuleHandleW failed".to_string());
    }
    let width = (rect.right - rect.left).max(0);
    let height = (rect.bottom - rect.top).max(0);
    let hwnd = unsafe {
        ffi::CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            ffi::WS_CHILD | ffi::WS_VISIBLE | ffi::WS_CLIPCHILDREN,
            rect.left,
            rect.top,
            width,
            height,
            parent,
            0,
            instance,
            std::ptr::null_mut(),
        )
    };
    if hwnd == 0 {
        return Err("CreateWindowExW failed".to_string());
    }
    let edit = unsafe {
        ffi::CreateWindowExW(
            0x0000_0200, // WS_EX_CLIENTEDGE
            edit_class.as_ptr(),
            std::ptr::null(),
            ffi::WS_CHILD
                | ffi::WS_VISIBLE
                | ffi::WS_VSCROLL
                | ffi::WS_TABSTOP
                | ffi::ES_MULTILINE
                | ffi::ES_AUTOVSCROLL
                | ffi::ES_READONLY,
            0,
            0,
            width,
            height,
            hwnd,
            0,
            instance,
            std::ptr::null_mut(),
        )
    };
    if edit == 0 {
        unsafe {
            ffi::DestroyWindow(hwnd);
        }
        return Err("could not create the preview text control".to_string());
    }
    // Subclass the edit so F1 opens the About screen.
    let context = Box::into_raw(Box::new(WindowContext { edit, text, colors }));
    unsafe {
        let old = ffi::SetWindowLongPtrW(edit, ffi::GWLP_WNDPROC, edit_proc as *const () as isize);
        ffi::SetWindowLongPtrW(edit, ffi::GWLP_USERDATA, old);
        ffi::SetWindowLongPtrW(hwnd, ffi::GWLP_USERDATA, context as isize);
    }
    resize_edit(hwnd, edit);
    Ok((hwnd, edit))
}

fn register_class() -> Result<(), String> {
    let class_name = window_class_name();
    let instance = win32_registry::current_module_handle().map_err(|e| e.to_string())?;
    let class = ffi::WNDCLASSW {
        style: 0,
        lpfnWndProc: Some(window_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: instance,
        hIcon: 0,
        hCursor: 0,
        hbrBackground: 0,
        lpszMenuName: std::ptr::null(),
        lpszClassName: class_name.as_ptr(),
    };
    let atom = unsafe { ffi::RegisterClassW(&class) };
    if atom == 0 {
        return Err("RegisterClassW failed".to_string());
    }
    Ok(())
}

fn apply_font(edit: ffi::HWND, font: ffi::HFONT) {
    if edit != 0 && font != 0 {
        unsafe {
            ffi::SendMessageW(edit, ffi::WM_SETFONT, font as ffi::WPARAM, 1);
        }
    }
}

fn resize_edit(parent: ffi::HWND, edit: ffi::HWND) {
    if edit == 0 {
        return;
    }
    let mut rect = ffi::RECT::default();
    let ok = unsafe { ffi::GetClientRect(parent, &mut rect) };
    if ok == 0 {
        return;
    }
    unsafe {
        ffi::SetWindowPos(
            edit,
            0,
            0,
            0,
            rect.right.max(0),
            rect.bottom.max(0),
            ffi::SWP_NOZORDER | ffi::SWP_NOACTIVATE,
        );
    }
}

unsafe extern "system" fn window_proc(
    hwnd: ffi::HWND,
    message: u32,
    wparam: ffi::WPARAM,
    lparam: ffi::LPARAM,
) -> ffi::LRESULT {
    let raw = ffi::GetWindowLongPtrW(hwnd, ffi::GWLP_USERDATA);
    let context = raw as *mut WindowContext;
    match message {
        ffi::WM_SIZE => {
            if !context.is_null() {
                resize_edit(hwnd, (*context).edit);
            }
            0
        }
        WM_APP_PREVIEW_READY => {
            if !context.is_null() {
                let text = (*context).text.lock().ok().and_then(|mut slot| slot.take());
                if let Some(text) = text {
                    set_window_text((*context).edit, &text);
                }
            }
            0
        }
        ffi::WM_DESTROY => {
            if !context.is_null() {
                ffi::SetWindowLongPtrW(hwnd, ffi::GWLP_USERDATA, 0);
                drop(Box::from_raw(context));
            }
            ffi::DefWindowProcW(hwnd, message, wparam, lparam)
        }
        ffi::WM_ERASEBKGND => 1,
        ffi::WM_CTLCOLORSTATIC => {
            // The read-only preview edit is a static for colour purposes; let
            // Explorer's colours show through a hollow (NULL) brush.
            if context.is_null() {
                ffi::DefWindowProcW(hwnd, message, wparam, lparam)
            } else {
                let colors = (*context)
                    .colors
                    .lock()
                    .map(|colors| *colors)
                    .unwrap_or_default();
                let dc = wparam as ffi::HDC;
                unsafe {
                    ffi::SetTextColor(dc, colors.text_color);
                    ffi::SetBkColor(dc, colors.background);
                    ffi::GetStockObject(ffi::NULL_BRUSH) as ffi::LRESULT
                }
            }
        }
        ffi::WM_KEYDOWN if wparam == ffi::VK_F1 => {
            show_about(hwnd);
            0
        }
        _ => ffi::DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe extern "system" fn edit_proc(
    hwnd: ffi::HWND,
    message: u32,
    wparam: ffi::WPARAM,
    lparam: ffi::LPARAM,
) -> ffi::LRESULT {
    if message == ffi::WM_KEYDOWN && wparam == ffi::VK_F1 {
        show_about(hwnd);
        return 0;
    }
    let raw = ffi::GetWindowLongPtrW(hwnd, ffi::GWLP_USERDATA);
    let previous: ffi::WNDPROC = std::mem::transmute(raw);
    match previous {
        Some(_) => ffi::CallWindowProcW(previous, hwnd, message, wparam, lparam),
        None => ffi::DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lf_text_is_converted_to_crlf() {
        // Every bare LF becomes CRLF...
        assert_eq!(to_crlf("a\nb\n\nend"), "a\r\nb\r\n\r\nend");
        // ...an existing CRLF is left untouched (no \r\r\n)...
        assert_eq!(to_crlf("x\r\ny"), "x\r\ny");
        // ...and text without newlines is unchanged.
        assert_eq!(to_crlf("solo"), "solo");
        // A lone CR is not a line break and must not gain an LF sibling.
        assert_eq!(to_crlf("a\rb"), "a\rb");
    }

    /// L3: the redundant PID kill path was removed, so stopping a preview after
    /// the worker finished must signal cancellation and join the thread without
    /// ever touching a PID. The job object's kill-on-close owns termination.
    #[test]
    fn stop_preview_is_safe_without_a_worker_pid() {
        let mut state = HandlerState::default();
        let finished = Arc::new(AtomicBool::new(false));
        let flag = finished.clone();
        state.thread = Some(std::thread::spawn(move || {
            flag.store(true, Ordering::Relaxed);
        }));
        state.stop_preview();
        assert!(state.cancel.load(Ordering::Relaxed), "must signal cancel");
        assert!(state.thread.is_none(), "stop_preview must drop the thread");
        assert!(finished.load(Ordering::Relaxed), "the thread must have run");
        // Idempotent with no job and no thread left.
        state.stop_preview();
        state.unload();
    }
}

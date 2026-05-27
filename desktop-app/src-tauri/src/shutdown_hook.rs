//! Windows shutdown / sign-off hook.
//!
//! Tauri's main window gets `WM_QUERYENDSESSION` from Windows when the user
//! logs off, restarts, or shuts down. We intercept it via a window-message
//! subclass and fire a callback so the rest of the app can send a graceful
//! "device_offline" webhook before Windows kills the process.

#[cfg(windows)]
pub use windows_impl::install;

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn install<F: Fn() + Send + Sync + 'static>(_handler: F) {
    // No-op on non-Windows targets.
}

#[cfg(windows)]
mod windows_impl {
    use std::sync::Arc;
    use std::sync::OnceLock;

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, FindWindowExW, SetWindowLongPtrW, GWLP_WNDPROC, WM_ENDSESSION,
        WM_QUERYENDSESSION,
    };

    type Handler = Arc<dyn Fn() + Send + Sync + 'static>;

    static HANDLER: OnceLock<Handler> = OnceLock::new();
    static ORIGINAL_PROC: OnceLock<isize> = OnceLock::new();

    /// Subclass the Tauri main window to fire `handler` when Windows sends
    /// `WM_QUERYENDSESSION`. Idempotent (the OnceLocks short-circuit a
    /// second install — only the first handler is installed).
    pub fn install<F>(handler: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        let _ = HANDLER.set(Arc::new(handler));

        // Find the Tauri window by its class name. Tauri 2 uses the "Window
        // Class" name "Tauri Window" for its main HWND. If FindWindowEx
        // returns null we bail out — the app keeps working, just without
        // the graceful shutdown signal.
        let class_name: Vec<u16> = "Tauri Window\0".encode_utf16().collect();
        let hwnd = unsafe {
            FindWindowExW(None, None, PCWSTR(class_name.as_ptr()), PCWSTR::null())
        };

        if let Ok(hwnd) = hwnd {
            if !hwnd.0.is_null() {
                unsafe {
                    let original = SetWindowLongPtrW(
                        hwnd,
                        GWLP_WNDPROC,
                        subclassed_proc as *const () as isize,
                    );
                    let _ = ORIGINAL_PROC.set(original);
                }
            }
        }
    }

    /// Subclassed WindowProc — intercepts shutdown messages, forwards everything
    /// else to the original proc.
    unsafe extern "system" fn subclassed_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_QUERYENDSESSION || msg == WM_ENDSESSION {
            if let Some(handler) = HANDLER.get() {
                handler();
            }
            // Fall through to the original proc so Windows continues the
            // shutdown handshake normally.
        }
        let original = ORIGINAL_PROC.get().copied().unwrap_or(0);
        if original == 0 {
            // Defensive: if we somehow lost the original, return 1 for
            // WM_QUERYENDSESSION (allow shutdown) and 0 otherwise.
            return LRESULT(if msg == WM_QUERYENDSESSION { 1 } else { 0 });
        }
        CallWindowProcW(
            Some(std::mem::transmute(original)),
            hwnd,
            msg,
            wparam,
            lparam,
        )
    }
}

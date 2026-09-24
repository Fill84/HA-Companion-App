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
pub fn install<R: tauri::Runtime, F: Fn() + Send + Sync + 'static>(
    _window: &tauri::Window<R>,
    _handler: F,
) {
    // No-op on non-Windows targets.
}

#[cfg(windows)]
mod windows_impl {
    use std::sync::Arc;
    use std::sync::OnceLock;

    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, SetWindowLongPtrW, GWLP_WNDPROC, WM_ENDSESSION, WM_QUERYENDSESSION,
    };

    type Handler = Arc<dyn Fn() + Send + Sync + 'static>;

    static HANDLER: OnceLock<Handler> = OnceLock::new();
    static ORIGINAL_PROC: OnceLock<isize> = OnceLock::new();

    /// Subclass the Tauri main window to fire `handler` when Windows sends
    /// `WM_QUERYENDSESSION`. Idempotent (the OnceLocks short-circuit a
    /// second install — only the first handler is installed).
    pub fn install<R: tauri::Runtime, F>(window: &tauri::Window<R>, handler: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        let _ = HANDLER.set(Arc::new(handler));

        if let Ok(native) = window.hwnd() {
            let hwnd = HWND(native.0);
            unsafe {
                let original =
                    SetWindowLongPtrW(hwnd, GWLP_WNDPROC, subclassed_proc as *const () as isize);
                let _ = ORIGINAL_PROC.set(original);
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
        // WM_QUERYENDSESSION may later be cancelled; only the confirmed
        // WM_ENDSESSION(true) means this instance is actually leaving.
        if msg == WM_ENDSESSION && wparam.0 != 0 {
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
            Some(std::mem::transmute::<
                isize,
                unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
            >(original)),
            hwnd,
            msg,
            wparam,
            lparam,
        )
    }
}

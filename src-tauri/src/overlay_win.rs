//! Windows-only overlay chrome helpers.
//!
//! Goal: Wispr-Flow look — only the pill is visible.
//!
//! On this machine WebView2 still paints an opaque white HWND rectangle even
//! with DefaultBackgroundColor A=0, so while listening we clip to a capsule
//! with SetWindowRgn. Dismiss always parks OFF-SCREEN before clearing that
//! region — clearing on-screen was the white flash.
//!
//! Click-through uses WS_EX_TRANSPARENT only (never leave WS_EX_LAYERED on
//! without a color key — that also paints white).

#![allow(dead_code)]

use tauri::WebviewWindow;

/// Charcoal RGB with A=0. If alpha fails we still never flash WebView2 white.
const CLEAR: tauri::window::Color = tauri::window::Color(18, 18, 18, 0);

/// Force window + webview background fully transparent (sync when on main thread).
pub fn clear_background(w: &WebviewWindow) {
    let _ = w.set_background_color(Some(CLEAR));
    let _ = w.set_shadow(false);
    strip_dwm_chrome(w);
    drop_layered(w);
    force_webview2_clear(w);
}

fn force_webview2_clear(w: &WebviewWindow) {
    #[cfg(windows)]
    {
        let _ = w.with_webview(|wv| unsafe {
            use webview2_com::Microsoft::Web::WebView2::Win32::{
                ICoreWebView2Controller2, COREWEBVIEW2_COLOR,
            };
            use windows::core::Interface;
            if let Ok(c2) = wv.controller().cast::<ICoreWebView2Controller2>() {
                let _ = c2.SetDefaultBackgroundColor(COREWEBVIEW2_COLOR {
                    A: 0,
                    R: 18,
                    G: 18,
                    B: 18,
                });
            }
        });
    }
    #[cfg(not(windows))]
    {
        let _ = w;
    }
}

/// Click-through without `WS_EX_LAYERED`.
pub fn set_click_through(w: &WebviewWindow, on: bool) {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, SWP_FRAMECHANGED,
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_EX_LAYERED, WS_EX_TRANSPARENT,
        };
        let Ok(hwnd) = w.hwnd() else { return };
        if !on {
            let _ = w.set_ignore_cursor_events(false);
        }
        unsafe {
            let cur = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            let mut next = cur & !WS_EX_LAYERED.0;
            if on {
                next |= WS_EX_TRANSPARENT.0;
            } else {
                next &= !WS_EX_TRANSPARENT.0;
            }
            if next != cur {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next as isize);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
            }
        }
        drop_layered(w);
    }
    #[cfg(not(windows))]
    {
        let _ = w.set_ignore_cursor_events(on);
    }
}

fn drop_layered(w: &WebviewWindow) {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_LAYERED,
        };
        let Ok(hwnd) = w.hwnd() else { return };
        unsafe {
            let cur = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            if cur & WS_EX_LAYERED.0 != 0 {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (cur & !WS_EX_LAYERED.0) as isize);
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = w;
    }
}

/// Clip HWND to a capsule. Must run while the window is still OFF-SCREEN —
/// applying this after a reveal is the first-frame white rectangle.
fn clip_capsule(w: &WebviewWindow) {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::RECT;
        use windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, SetWindowRgn};
        use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
        let Ok(hwnd) = w.hwnd() else { return };
        unsafe {
            let mut rc = RECT::default();
            if GetClientRect(hwnd, &mut rc).is_err() {
                return;
            }
            let width = rc.right - rc.left;
            let height = rc.bottom - rc.top;
            if width <= 2 || height <= 2 {
                return;
            }
            // Inset 2px — GDI regions are aliased; without inset WebView2 white
            // peeks through at capsule corners (top-left/right fringe).
            const INSET: i32 = 2;
            let inner_w = width - INSET * 2;
            let inner_h = height - INSET * 2;
            if inner_w <= 2 || inner_h <= 2 {
                return;
            }
            // True capsule: ellipse diameter == inner height on both ends.
            let region = CreateRoundRectRgn(
                INSET,
                INSET,
                width - INSET,
                height - INSET,
                inner_h,
                inner_h,
            );
            if region.is_invalid() {
                return;
            }
            // Ownership transfers to the system. Never redraw=true (white flash).
            SetWindowRgn(hwnd, Some(region), false);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = w;
    }
}

/// Clip HWND to a capsule while listening. WebView2 still paints a white
/// rectangle without this — CSS radius alone is not enough on this machine.
/// Always clip while off-screen, then reveal.
pub fn apply_pill_region(w: &WebviewWindow) {
    clear_background(w);
    clip_capsule(w);
    clear_background(w);
}

/// Drop any leftover GDI clip (from older builds) without redrawing on-screen.
fn clear_any_region(w: &WebviewWindow) {
    #[cfg(windows)]
    {
        use windows::Win32::Graphics::Gdi::{SetWindowRgn, HRGN};
        let Ok(hwnd) = w.hwnd() else { return };
        unsafe {
            SetWindowRgn(hwnd, Some(HRGN(std::ptr::null_mut())), false);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = w;
    }
}

pub fn clear_pill_region(w: &WebviewWindow) {
    clear_any_region(w);
    clear_background(w);
}

pub fn clear_window_region(w: &WebviewWindow) {
    clear_pill_region(w);
}

/// Park off-screen FIRST, then keep the capsule clip so the next reveal is
/// already pill-shaped (no white rectangle on the first frame).
pub fn park_idle(w: &WebviewWindow) {
    #[cfg(windows)]
    {
        use tauri::{LogicalSize, Size};
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOSIZE, SWP_SHOWWINDOW,
        };
        // Move off-screen BEFORE any size/chrome change — those flash white
        // if they run while the HWND is still on the desktop.
        if let Ok(hwnd) = w.hwnd() {
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    -40_000,
                    -40_000,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
        }
        let _ = w.set_size(Size::Logical(LogicalSize {
            width: 148.0,
            height: 36.0,
        }));
        // Keep the capsule while idle — next hotkey only has to move on-screen.
        clip_capsule(w);
        set_click_through(w, true);
        clear_background(w);
    }
    #[cfg(not(windows))]
    {
        use tauri::{LogicalPosition, LogicalSize, Position, Size};
        clear_background(w);
        let _ = w.set_size(Size::Logical(LogicalSize {
            width: 148.0,
            height: 36.0,
        }));
        let _ = w.set_position(Position::Logical(LogicalPosition {
            x: -40_000.0,
            y: -40_000.0,
        }));
        let _ = w.show();
        set_click_through(w, true);
        clear_background(w);
    }
}

/// Reveal the listening pill. Size + clip WHILE still off-screen, then one move.
pub fn reveal_listening(w: &WebviewWindow, x: i32, y: i32) {
    use tauri::{LogicalSize, Size};
    clear_background(w);
    let _ = w.set_size(Size::Logical(LogicalSize {
        width: 148.0,
        height: 36.0,
    }));
    clip_capsule(w);
    set_click_through(w, true);
    move_topmost(w, x, y);
}

fn strip_dwm_chrome(w: &WebviewWindow) {
    #[cfg(windows)]
    {
        use windows::Win32::Graphics::Dwm::{
            DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMWA_BORDER_COLOR,
            DWMWA_COLOR_NONE, DWMWA_NCRENDERING_POLICY, DWMWA_SYSTEMBACKDROP_TYPE,
            DWMWA_TRANSITIONS_FORCEDISABLED, DWMWA_WINDOW_CORNER_PREFERENCE, DWMSBT_NONE,
            DWMNCRP_DISABLED, DWMWCP_DONOTROUND,
        };
        use windows::Win32::UI::Controls::MARGINS;

        let Ok(hwnd) = w.hwnd() else { return };
        unsafe {
            let border = DWMWA_COLOR_NONE;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_BORDER_COLOR,
                &border as *const _ as *const _,
                std::mem::size_of_val(&border) as u32,
            );

            let corner = DWMWCP_DONOTROUND;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &corner as *const _ as *const _,
                std::mem::size_of_val(&corner) as u32,
            );

            let backdrop = DWMSBT_NONE;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE,
                &backdrop as *const _ as *const _,
                std::mem::size_of_val(&backdrop) as u32,
            );

            let disable_transitions: i32 = 1;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_TRANSITIONS_FORCEDISABLED,
                &disable_transitions as *const _ as *const _,
                std::mem::size_of_val(&disable_transitions) as u32,
            );

            let ncrp = DWMNCRP_DISABLED;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_NCRENDERING_POLICY,
                &ncrp as *const _ as *const _,
                std::mem::size_of_val(&ncrp) as u32,
            );

            let margins = MARGINS {
                cxLeftWidth: -1,
                cxRightWidth: -1,
                cyTopHeight: -1,
                cyBottomHeight: -1,
            };
            let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = w;
    }
}

/// Move the overlay with a single `SetWindowPos`.
pub fn move_topmost(w: &WebviewWindow, x: i32, y: i32) {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOSIZE, SWP_SHOWWINDOW,
        };
        let Ok(hwnd) = w.hwnd() else { return };
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (w, x, y);
    }
}

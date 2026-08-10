//! Windows-only overlay chrome helpers.
//! Keep the overlay webview truly transparent so CSS `border-radius` can
//! anti-alias the listening pill. Do NOT use SetWindowRgn / CreateRoundRectRgn
//! — GDI regions are aliased and make pill edges look jagged.
//!
//! Win11 DWM still draws a 1px system border + auto corner rounding on small
//! undecorated HWNDs; that reads as a white halo around the CSS pill on dark
//! desktops. Strip that chrome every time we clear/show.

#![allow(dead_code)]

use tauri::WebviewWindow;

/// Pill charcoal with alpha 0.
///
/// When WebView2 honors A=0, corners stay transparent (smooth CSS radius).
/// If transparency fails and the color is forced opaque, the flash matches the
/// pill instead of WebView2's default white.
const CLEAR: tauri::window::Color = tauri::window::Color(18, 18, 18, 0);

/// Force window + webview background fully transparent (sync when on main thread).
pub fn clear_background(w: &WebviewWindow) {
    let _ = w.set_background_color(Some(CLEAR));
    let _ = w.set_shadow(false);
    strip_dwm_chrome(w);
    force_webview2_clear(w);
}

/// Hit WebView2 `DefaultBackgroundColor` directly — more reliable than hoping
/// the queued Tauri message lands before the first paint after a geometry show.
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

/// Remove Win11 DWM border / rounded-frame fringe that outlines transparent overlays.
fn strip_dwm_chrome(w: &WebviewWindow) {
    #[cfg(windows)]
    {
        use windows::Win32::Graphics::Dwm::{
            DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMWA_BORDER_COLOR,
            DWMWA_COLOR_NONE, DWMWA_NCRENDERING_POLICY, DWMWA_SYSTEMBACKDROP_TYPE,
            DWMWA_TRANSITIONS_FORCEDISABLED, DWMWA_WINDOW_CORNER_PREFERENCE, DWMSBT_NONE,
            DWMWCP_DONOTROUND, DWMNCRP_DISABLED,
        };
        use windows::Win32::UI::Controls::MARGINS;

        let Ok(hwnd) = w.hwnd() else { return };
        unsafe {
            // No system 1px border (shows as white outline on dark backgrounds).
            let border = DWMWA_COLOR_NONE;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_BORDER_COLOR,
                &border as *const _ as *const _,
                std::mem::size_of_val(&border) as u32,
            );

            // Let CSS border-radius own the shape — OS rounding adds a light fringe.
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

            let disable_transitions: i32 = 1; // TRUE
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_TRANSITIONS_FORCEDISABLED,
                &disable_transitions as *const _ as *const _,
                std::mem::size_of_val(&disable_transitions) as u32,
            );

            // Disable non-client DWM rendering that can stroke a light edge.
            let ncrp = DWMNCRP_DISABLED;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_NCRENDERING_POLICY,
                &ncrp as *const _ as *const _,
                std::mem::size_of_val(&ncrp) as u32,
            );

            // Extend a fully transparent frame so DWM doesn't paint opaque chrome.
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

/// Ensure no HWND region clip (smooth CSS corners) and clear chrome.
/// Call AFTER size/position are applied.
pub fn apply_pill_region(w: &WebviewWindow) {
    clear_pill_region(w);
}

/// Remove any leftover GDI clip region (idle / park off-screen / after resize).
pub fn clear_pill_region(w: &WebviewWindow) {
    #[cfg(windows)]
    {
        use windows::Win32::Graphics::Gdi::SetWindowRgn;
        if let Ok(hwnd) = w.hwnd() {
            let _ = unsafe { SetWindowRgn(hwnd, None, true) };
        }
    }
    #[cfg(not(windows))]
    {
        let _ = w;
    }
    clear_background(w);
}

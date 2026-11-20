//! Floating suggestion chip next to the caret. Non-activating, so the app being typed in
//! keeps focus (and its caret) while the user clicks the chip.
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

const LABEL: &str = "autocorrect-chip";
const CHIP_W: f64 = 300.0;
const CHIP_H: f64 = 40.0;
const BELOW_CARET_PX: i32 = 28;

static VISIBLE: AtomicBool = AtomicBool::new(false);
static RECT_X: AtomicI32 = AtomicI32::new(0);
static RECT_Y: AtomicI32 = AtomicI32::new(0);
static RECT_W: AtomicI32 = AtomicI32::new(0);
static RECT_H: AtomicI32 = AtomicI32::new(0);

/// Is this screen point on the visible chip? The mouse hook uses it so that clicking the chip
/// does not look like "the caret moved" and wipe the suggestion it is about to apply.
pub fn contains(x: i32, y: i32) -> bool {
    if !VISIBLE.load(Ordering::SeqCst) {
        return false;
    }
    let (rx, ry) = (RECT_X.load(Ordering::SeqCst), RECT_Y.load(Ordering::SeqCst));
    x >= rx && x < rx + RECT_W.load(Ordering::SeqCst) && y >= ry && y < ry + RECT_H.load(Ordering::SeqCst)
}

fn window(app: &AppHandle) -> Option<WebviewWindow> {
    if let Some(w) = app.get_webview_window(LABEL) {
        return Some(w);
    }
    let built = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html?window=autocorrect-chip".into()))
        .title("MaxSpeech Autocorrect")
        .inner_size(CHIP_W, CHIP_H)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        .focused(false)
        .shadow(true)
        .build();
    match built {
        Ok(w) => {
            crate::enhancer::make_no_activate(&w);
            Some(w)
        }
        Err(e) => {
            log::error!("Could not create autocorrect chip window: {e}");
            None
        }
    }
}

/// Build the (hidden) window ahead of time so the first suggestion appears instantly.
pub fn warm_up(app: &AppHandle) {
    let _ = window(app);
}

fn place(app: &AppHandle, w: &WebviewWindow, target: isize) {
    let (ax, ay) = crate::enhancer::caret_anchor(target).unwrap_or_else(|| {
        app.cursor_position().map(|p| (p.x as i32, p.y as i32)).unwrap_or((400, 400))
    });
    let monitor = w
        .monitor_from_point(ax as f64, ay as f64)
        .ok()
        .flatten()
        .or_else(|| w.primary_monitor().ok().flatten());
    let scale = monitor.as_ref().map_or(1.0, |m| m.scale_factor());
    let width = (CHIP_W * scale).round() as i32;
    let height = (CHIP_H * scale).round() as i32;
    let mut x = ax;
    let mut y = ay + (f64::from(BELOW_CARET_PX) * scale).round() as i32;
    if let Some(m) = &monitor {
        let (mp, ms) = (m.position(), m.size());
        x = x.clamp(mp.x, (mp.x + ms.width as i32 - width).max(mp.x));
        if y + height > mp.y + ms.height as i32 {
            // No room below: sit above the line instead.
            y = ay - height - (8.0 * scale).round() as i32;
        }
        y = y.clamp(mp.y, (mp.y + ms.height as i32 - height).max(mp.y));
    }
    let _ = w.set_size(PhysicalSize::new(width as u32, height as u32));
    let _ = w.set_position(PhysicalPosition::new(x, y));
    RECT_X.store(x, Ordering::SeqCst);
    RECT_Y.store(y, Ordering::SeqCst);
    RECT_W.store(width, Ordering::SeqCst);
    RECT_H.store(height, Ordering::SeqCst);
}

pub fn show(app: &AppHandle, target: isize, payload: serde_json::Value) {
    let Some(w) = window(app) else { return };
    place(app, &w, target);
    let _ = w.show();
    let _ = w.set_always_on_top(true);
    VISIBLE.store(true, Ordering::SeqCst);
    let _ = app.emit_to(LABEL, "autocorrect-chip-show", payload);
}

pub fn hide(app: &AppHandle) {
    if !VISIBLE.swap(false, Ordering::SeqCst) {
        return;
    }
    let _ = app.emit_to(LABEL, "autocorrect-chip-hide", ());
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.hide();
    }
}

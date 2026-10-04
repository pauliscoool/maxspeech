//! Ctrl+Shift+E "enhance selection" widget: copies the selected text (or all
//! of it), streams an AI rewrite into a floating window, then pastes it back.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
};

use crate::inject;
use crate::pipeline::{self, tone};
use crate::store::Store;

const LABEL: &str = "enhancer";
const WIDGET_W: f64 = 352.0;
const WIDGET_H: f64 = 264.0;
const GAP_PX: i32 = 14;
const DEFAULT_FORMALITY: &str = "neutral";

struct Session {
    original: String,
    result: String,
    error: Option<String>,
    target_hwnd: isize,
    suggested: Option<&'static str>,
}

static SESSION: Mutex<Session> = Mutex::new(Session {
    original: String::new(),
    result: String::new(),
    error: None,
    target_hwnd: 0,
    suggested: None,
});
/// Bumped on every new stream / close so stale streams stop emitting.
static RUN: AtomicU64 = AtomicU64::new(0);
/// Run id the user pressed Stop on (keeps the partial text).
static STOP_RUN: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Serialize)]
struct Update {
    run: u64,
    text: String,
    state: &'static str,
    error: Option<String>,
}

#[derive(Serialize)]
pub struct SessionInfo {
    original: String,
    formality: String,
    error: Option<String>,
}

fn lock_session() -> std::sync::MutexGuard<'static, Session> {
    SESSION.lock().unwrap_or_else(|e| e.into_inner())
}

fn normalize_formality(raw: &str) -> &'static str {
    match raw {
        "casual" => "casual",
        "professional" => "professional",
        "formal" => "formal",
        "claude_code" => "claude_code",
        "cursor_codex" => "cursor_codex",
        _ => DEFAULT_FORMALITY,
    }
}

/// Coding agents are recognised by app or window title (Claude Code runs inside a terminal).
fn detect_agent_mode() -> Option<&'static str> {
    let app = crate::context::get_foreground_app()?;
    let exe = app.exe.to_lowercase();
    let title = app.title.to_lowercase();
    if title.contains("claude code") || (exe.starts_with("claude") && title.contains("code")) {
        Some("claude_code")
    } else if exe.starts_with("cursor") || exe.starts_with("codex") || title.contains("codex") {
        Some("cursor_codex")
    } else {
        None
    }
}

/// Modes that rewrite the text into a coding-agent prompt instead of editing its tone.
fn is_prompt_mode(mode: &str) -> bool {
    matches!(mode, "claude_code" | "cursor_codex")
}

fn stored_formality(app: &AppHandle) -> String {
    app.state::<Store>()
        .get_setting("enhancer_formality")
        .ok()
        .flatten()
        .map(|f| normalize_formality(&f).to_string())
        .unwrap_or_else(|| DEFAULT_FORMALITY.to_string())
}

#[cfg(windows)]
fn foreground_hwnd() -> isize {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    unsafe { GetForegroundWindow().0 as isize }
}

#[cfg(not(windows))]
fn foreground_hwnd() -> isize {
    0
}

/// Top-left of the text caret in screen pixels, when the app exposes one.
#[cfg(windows)]
fn caret_anchor(hwnd: isize) -> Option<(i32, i32)> {
    use windows::Win32::Foundation::{HWND, POINT};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetGUIThreadInfo, GetWindowThreadProcessId, GUITHREADINFO,
    };
    if hwnd == 0 {
        return None;
    }
    unsafe {
        let tid = GetWindowThreadProcessId(HWND(hwnd as *mut _), None);
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        GetGUIThreadInfo(tid, &mut info).ok()?;
        if info.hwndCaret.is_invalid() {
            return None;
        }
        let mut pt = POINT {
            x: info.rcCaret.left,
            y: info.rcCaret.top,
        };
        if !ClientToScreen(info.hwndCaret, &mut pt).as_bool() {
            return None;
        }
        Some((pt.x, pt.y))
    }
}

#[cfg(not(windows))]
fn caret_anchor(_hwnd: isize) -> Option<(i32, i32)> {
    None
}

/// Never steal focus from the app being edited — its selection must survive.
#[cfg(windows)]
fn make_no_activate(w: &tauri::WebviewWindow) {
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };
    let Ok(hwnd) = w.hwnd() else { return };
    unsafe {
        let cur = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            (cur | WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize,
        );
        let corner = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as *const _,
            std::mem::size_of_val(&corner) as u32,
        );
    }
}

#[cfg(not(windows))]
fn make_no_activate(_w: &tauri::WebviewWindow) {}

/// Blur whatever is behind the widget; the CSS tint keeps it readable when
/// the OS can't (older Windows just gets the tint).
#[cfg(windows)]
fn apply_glass(app: &AppHandle, w: &tauri::WebviewWindow) {
    // Tint follows the app theme so the glass matches Dark / Gray / Light.
    let tint = match app
        .state::<Store>()
        .get_setting("ui_theme")
        .ok()
        .flatten()
        .as_deref()
    {
        Some("light") => (238, 240, 244, 90),
        Some("gray") => (40, 40, 44, 80),
        _ => (6, 6, 8, 80),
    };
    if let Err(e) = window_vibrancy::apply_acrylic(w, Some(tint)) {
        log::warn!("Acrylic backdrop unavailable: {e}");
    }
}

#[cfg(not(windows))]
fn apply_glass(_app: &AppHandle, _w: &tauri::WebviewWindow) {}

/// Place the widget above the caret (or mouse pointer), clamped to its monitor.
fn position_widget(app: &AppHandle, w: &tauri::WebviewWindow, target: isize) {
    let (ax, ay) = caret_anchor(target).unwrap_or_else(|| {
        app.cursor_position()
            .map(|p| (p.x as i32, p.y as i32))
            .unwrap_or((400, 400))
    });
    let monitor = w
        .monitor_from_point(ax as f64, ay as f64)
        .ok()
        .flatten()
        .or_else(|| w.primary_monitor().ok().flatten());
    let scale = monitor.as_ref().map(|m| m.scale_factor()).unwrap_or(1.0);
    let width = (WIDGET_W * scale).round() as i32;
    let height = (WIDGET_H * scale).round() as i32;

    let mut x = ax - width / 2;
    let mut y = ay - height - GAP_PX;
    if let Some(m) = &monitor {
        let mp = m.position();
        let ms = m.size();
        x = x.clamp(mp.x, (mp.x + ms.width as i32 - width).max(mp.x));
        if y < mp.y {
            // No room above — drop below the anchor instead.
            y = ay + GAP_PX * 3;
        }
        y = y.clamp(mp.y, (mp.y + ms.height as i32 - height).max(mp.y));
    }

    let _ = w.set_size(PhysicalSize::new(width as u32, height as u32));
    let _ = w.set_position(PhysicalPosition::new(x, y));
}

fn show_widget(app: &AppHandle, target: isize) {
    if let Some(w) = app.get_webview_window(LABEL) {
        apply_glass(app, &w);
        position_widget(app, &w, target);
        let _ = w.show();
        let _ = w.set_always_on_top(true);
        let _ = app.emit_to(LABEL, "enhancer:open", ());
        return;
    }
    let built = WebviewWindowBuilder::new(
        app,
        LABEL,
        WebviewUrl::App("index.html?window=enhancer".into()),
    )
    .title("MaxSpeech Enhance")
    .inner_size(WIDGET_W, WIDGET_H)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .visible(false)
    .focused(false)
    .shadow(true)
    .transparent(true)
    .build();
    match built {
        Ok(w) => {
            make_no_activate(&w);
            apply_glass(app, &w);
            position_widget(app, &w, target);
            let _ = w.show();
        }
        Err(e) => log::error!("Could not create enhancer window: {e}"),
    }
}

/// Ctrl+Shift+E entry point (runs off the hotkey thread).
pub fn trigger(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let signed_in = pipeline::is_signed_in(&app.state::<Store>());
        if !signed_in {
            return;
        }
        let target = foreground_hwnd();
        // Pressing the hotkey while the widget itself is focused must not copy from it.
        if let Some(w) = app.get_webview_window(LABEL) {
            if w.hwnd().map(|h| h.0 as isize).ok() == Some(target) && target != 0 {
                return;
            }
        }

        let suggested = detect_agent_mode();
        let captured = inject::capture_selection();
        RUN.fetch_add(1, Ordering::SeqCst);
        {
            let mut s = lock_session();
            s.result.clear();
            s.target_hwnd = target;
            s.suggested = suggested;
            match captured {
                Ok(text) => {
                    s.original = text;
                    s.error = None;
                }
                Err(e) => {
                    s.original.clear();
                    s.error = Some(e.to_string());
                }
            }
        }
        let ui = app.clone();
        let _ = app.run_on_main_thread(move || show_widget(&ui, target));
    });
}

#[tauri::command]
pub fn enhancer_session(app: AppHandle) -> SessionInfo {
    let s = lock_session();
    SessionInfo {
        original: s.original.clone(),
        formality: s.suggested.map(String::from).unwrap_or_else(|| stored_formality(&app)),
        error: s.error.clone(),
    }
}

#[tauri::command]
pub async fn enhancer_run(app: AppHandle, formality: String) -> Result<u64, String> {
    let formality = normalize_formality(&formality);
    let original = lock_session().original.clone();
    if original.trim().is_empty() {
        return Err("No text to enhance".into());
    }

    let store = app.state::<Store>();
    // Agent prompt modes are picked per app, so they never become the saved default.
    if !is_prompt_mode(formality) {
        let _ = store.set_setting("enhancer_formality", formality);
    }
    let dict_terms: Vec<String> = store
        .get_dictionary()
        .unwrap_or_default()
        .into_iter()
        .map(|w| w.word)
        .collect();

    let run = RUN.fetch_add(1, Ordering::SeqCst) + 1;
    lock_session().result.clear();

    tauri::async_runtime::spawn(async move {
        let emit = |text: &str, state: &'static str, error: Option<String>| {
            let _ = app.emit_to(
                LABEL,
                "enhancer:update",
                Update {
                    run,
                    text: text.to_string(),
                    state,
                    error,
                },
            );
        };
        let superseded = move || RUN.load(Ordering::SeqCst) != run;
        let cancelled = move || superseded() || STOP_RUN.load(Ordering::SeqCst) == run;

        emit("", "streaming", None);
        let on_text = |text: &str| {
            if !superseded() {
                lock_session().result = text.to_string();
                emit(text, "streaming", None);
            }
        };
        let streamed = if is_prompt_mode(formality) {
            tone::stream_enhance_prompt(&original, formality, &dict_terms, on_text, cancelled).await
        } else {
            tone::stream_enhance_selection(&original, formality, &dict_terms, on_text, cancelled)
                .await
        };

        if superseded() {
            return;
        }
        match streamed {
            Ok(text) => {
                let text = text.trim().to_string();
                lock_session().result = text.clone();
                let state = if STOP_RUN.load(Ordering::SeqCst) == run {
                    "stopped"
                } else {
                    "done"
                };
                emit(&text, state, None);
            }
            Err(e) => {
                log::warn!("Enhancer stream failed: {e}");
                let partial = lock_session().result.clone();
                emit(&partial, "error", Some(e.to_string()));
            }
        }
    });

    Ok(run)
}

#[tauri::command]
pub fn enhancer_stop() {
    STOP_RUN.store(RUN.load(Ordering::SeqCst), Ordering::SeqCst);
}

#[tauri::command]
pub fn enhancer_close(app: AppHandle) {
    RUN.fetch_add(1, Ordering::SeqCst);
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.hide();
    }
}

#[tauri::command]
pub async fn enhancer_replace(app: AppHandle) -> Result<(), String> {
    let (text, target) = {
        let s = lock_session();
        (s.result.clone(), s.target_hwnd)
    };
    if text.trim().is_empty() {
        return Err("Nothing to paste yet".into());
    }
    RUN.fetch_add(1, Ordering::SeqCst);
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.hide();
    }
    tokio::task::spawn_blocking(move || {
        inject::focus_window(target);
        inject::inject_text(&text)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

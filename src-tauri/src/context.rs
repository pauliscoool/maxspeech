#[derive(Debug, Clone)]
pub struct ForegroundApp {
    pub exe: String,
    pub title: String,
}

#[cfg(windows)]
pub fn get_foreground_app() -> Option<ForegroundApp> {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    use windows::Win32::UI::WindowsAndMessaging::GetWindowTextW;
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};
    use windows::Win32::System::ProcessStatus::GetModuleFileNameExW;

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0 == std::ptr::null_mut() {
            return None;
        }

        let mut title_buf = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut title_buf);
        let title = String::from_utf16_lossy(&title_buf[..len as usize]);

        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let exe = if pid != 0 {
            if let Ok(process) = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid) {
                let mut exe_buf = [0u16; 512];
                let len = GetModuleFileNameExW(Some(process), None, &mut exe_buf);
                let full_path = String::from_utf16_lossy(&exe_buf[..len as usize]);
                full_path
                    .rsplit('\\')
                    .next()
                    .unwrap_or(&full_path)
                    .to_lowercase()
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        Some(ForegroundApp { exe, title })
    }
}

#[cfg(target_os = "macos")]
pub fn get_foreground_app() -> Option<ForegroundApp> {
    use std::process::Command;

    let name = Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to get name of first application process whose frontmost is true",
        ])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())?;

    let title = Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to get title of first window of (first application process whose frontmost is true)",
        ])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    // Normalize to something like "Safari.app" for friendly naming.
    let exe = if name.to_lowercase().ends_with(".app") {
        name.to_lowercase()
    } else {
        format!("{}.app", name.to_lowercase())
    };

    Some(ForegroundApp { exe, title })
}

#[cfg(target_os = "linux")]
pub fn get_foreground_app() -> Option<ForegroundApp> {
    use std::process::Command;

    // Wayland typically cannot query the active window without compositor portals.
    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        && std::env::var_os("DISPLAY").is_none()
    {
        return None;
    }

    let id_out = Command::new("xprop")
        .args(["-root", "_NET_ACTIVE_WINDOW"])
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let id_str = String::from_utf8_lossy(&id_out.stdout);
    // Typical: `_NET_ACTIVE_WINDOW(WINDOW): window id # 0x3c00007`
    let win_id = id_str
        .split('#')
        .nth(1)?
        .split_whitespace()
        .next()?
        .trim()
        .to_string();
    if win_id == "0x0" || win_id.is_empty() {
        return None;
    }

    let title = Command::new("xprop")
        .args(["-id", &win_id, "WM_NAME"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| parse_xprop_string(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default();

    let wm_class = Command::new("xprop")
        .args(["-id", &win_id, "WM_CLASS"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();

    // WM_CLASS(STRING) = "code", "Code" — prefer instance then class.
    let exe = wm_class
        .split('=')
        .nth(1)
        .map(|rest| {
            rest.split(',')
                .map(|p| p.trim().trim_matches('"').to_lowercase())
                .find(|p| !p.is_empty())
                .unwrap_or_default()
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into());

    Some(ForegroundApp { exe, title })
}

#[cfg(target_os = "linux")]
fn parse_xprop_string(raw: &str) -> Option<String> {
    // WM_NAME(STRING) = "Title"  or  WM_NAME(COMPOUND_TEXT) = "Title"
    let after = raw.split('=').nth(1)?;
    let s = after.trim().trim_matches('"').trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub fn get_foreground_app() -> Option<ForegroundApp> {
    None
}

/// Human-readable app label from an exe / bundle / binary name.
pub fn friendly_app_name(exe: &str) -> String {
    let lower = exe.to_lowercase();
    let base = lower.rsplit(['\\', '/']).next().unwrap_or(&lower);
    match base {
        "cursor.exe" | "cursor" | "cursor.app" => "Cursor".into(),
        "code.exe" | "code" | "code.app" | "visual studio code.app" => "VS Code".into(),
        "slack.exe" | "slack" | "slack.app" => "Slack".into(),
        "discord.exe" | "discord" | "discord.app" => "Discord".into(),
        "outlook.exe" | "microsoft outlook.app" => "Outlook".into(),
        "winword.exe" | "microsoft word.app" => "Word".into(),
        "excel.exe" | "microsoft excel.app" => "Excel".into(),
        "powerpnt.exe" | "microsoft powerpoint.app" => "PowerPoint".into(),
        "notion.exe" | "notion" | "notion.app" => "Notion".into(),
        "chrome.exe" | "google-chrome" | "chrome" | "google chrome.app" | "chromium" => {
            "Chrome".into()
        }
        "msedge.exe" | "microsoft edge.app" | "microsoft-edge" => "Edge".into(),
        "firefox.exe" | "firefox" | "firefox.app" => "Firefox".into(),
        "brave.exe" | "brave" | "brave-browser" | "brave browser.app" => "Brave".into(),
        "spotify.exe" | "spotify" | "spotify.app" => "Spotify".into(),
        "teams.exe" | "ms-teams.exe" | "microsoft teams.app" | "teams" => "Teams".into(),
        "notepad.exe" | "gedit" | "kate" | "mousepad" | "textedit.app" => "Text Editor".into(),
        "windowsterminal.exe" | "gnome-terminal" | "konsole" | "alacritty" | "kitty"
        | "wezterm" | "terminal.app" | "iterm2.app" => "Terminal".into(),
        "figma.exe" | "figma" | "figma.app" => "Figma".into(),
        "obsidian.exe" | "obsidian" | "obsidian.app" => "Obsidian".into(),
        "telegram.exe" | "telegram" | "telegram.app" | "telegram-desktop" => "Telegram".into(),
        "whatsapp.exe" | "whatsapp" | "whatsapp.app" => "WhatsApp".into(),
        "zoom.exe" | "zoom" | "zoom.us.app" => "Zoom".into(),
        "safari.app" => "Safari".into(),
        "mail.app" => "Mail".into(),
        "notes.app" => "Notes".into(),
        "messages.app" => "Messages".into(),
        "" | "unknown" => "Unknown".into(),
        other => {
            let name = other
                .trim_end_matches(".exe")
                .trim_end_matches(".app")
                .replace(['-', '_'], " ");
            let mut chars = name.chars();
            match chars.next() {
                Some(c) => {
                    let mut s = c.to_uppercase().collect::<String>();
                    s.push_str(chars.as_str());
                    s.split_whitespace()
                        .map(|w| {
                            let mut c = w.chars();
                            match c.next() {
                                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                                None => String::new(),
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                }
                None => "Unknown".into(),
            }
        }
    }
}

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

#[cfg(not(windows))]
pub fn get_foreground_app() -> Option<ForegroundApp> {
    None
}

/// Human-readable app label from an exe name (no .exe).
pub fn friendly_app_name(exe: &str) -> String {
    let lower = exe.to_lowercase();
    let base = lower.rsplit(['\\', '/']).next().unwrap_or(&lower);
    match base {
        "cursor.exe" => "Cursor".into(),
        "code.exe" => "VS Code".into(),
        "slack.exe" => "Slack".into(),
        "discord.exe" => "Discord".into(),
        "outlook.exe" => "Outlook".into(),
        "winword.exe" => "Word".into(),
        "excel.exe" => "Excel".into(),
        "powerpnt.exe" => "PowerPoint".into(),
        "notion.exe" => "Notion".into(),
        "chrome.exe" => "Chrome".into(),
        "msedge.exe" => "Edge".into(),
        "firefox.exe" => "Firefox".into(),
        "brave.exe" => "Brave".into(),
        "spotify.exe" => "Spotify".into(),
        "teams.exe" | "ms-teams.exe" => "Teams".into(),
        "notepad.exe" => "Notepad".into(),
        "windowsterminal.exe" => "Terminal".into(),
        "figma.exe" => "Figma".into(),
        "obsidian.exe" => "Obsidian".into(),
        "telegram.exe" => "Telegram".into(),
        "whatsapp.exe" => "WhatsApp".into(),
        "zoom.exe" => "Zoom".into(),
        "" | "unknown" => "Unknown".into(),
        other => {
            let name = other.trim_end_matches(".exe").replace(['-', '_'], " ");
            let mut chars = name.chars();
            match chars.next() {
                Some(c) => {
                    let mut s = c.to_uppercase().collect::<String>();
                    s.push_str(chars.as_str());
                    // Title-case remaining words
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

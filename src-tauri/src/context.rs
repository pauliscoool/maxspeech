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

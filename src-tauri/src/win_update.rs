//! Windows in-app update: download the NSIS installer, then hand off to a
//! detached reinstaller so MaxSpeech can fully quit before files are replaced.
//!
//! Tauri's built-in `install()` ShellExecutes the setup while this process is
//! still alive (and often still in a WebView2 job). The installer then cannot
//! overwrite `maxspeech.exe`, or it gets killed when we exit. The reinstaller
//! is a copy of this binary with a different name so NSIS will not task-kill it.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

pub const WEBSITE_WINDOWS: &str = "https://maxspeech.vercel.app/windows";
pub const WEBSITE_INSTALLER: &str =
    "https://maxspeech.vercel.app/downloads/MaxSpeech_x64-setup.exe";
pub const UPDATE_FAILED_MESSAGE: &str = "Update failed. Please go to https://maxspeech.vercel.app and install the latest version. If that doesn't work, this issue will be resolved soon.";

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
#[cfg(windows)]
const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

/// If this process was launched as the reinstaller, run that path and exit.
pub fn run_if_reinstaller() {
    let args: Vec<String> = std::env::args().collect();
    if !args.iter().any(|a| a == "--apply-update") {
        return;
    }
    let code = match apply_update_helper(&args) {
        Ok(()) => 0,
        Err(e) => {
            append_update_log(&format!("reinstaller failed: {e}"));
            show_update_failed_dialog();
            let _ = open_website();
            1
        }
    };
    std::process::exit(code);
}

/// Hosts we will ever download an installer from. Anything else is refused
/// before a single byte is fetched — the URL otherwise comes straight from
/// the website manifest / GitHub API response, both of which are untrusted
/// network input.
const ALLOWED_INSTALLER_HOSTS: &[&str] = &["maxspeech.vercel.app", "github.com"];

fn host_is_allowed(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    if parsed.scheme() != "https" {
        return false;
    }
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    ALLOWED_INSTALLER_HOSTS.contains(&host.as_str())
        || host.ends_with(".githubusercontent.com")
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[tauri::command]
pub async fn download_and_run_installer(
    app: AppHandle,
    url: String,
    sha256: Option<String>,
) -> Result<(), String> {
    download_and_run_installer_inner(app, url, sha256).await
}

async fn download_and_run_installer_inner(
    app: AppHandle,
    url: String,
    expected_sha256: Option<String>,
) -> Result<(), String> {
    let mut urls = installer_url_fallbacks(&url);
    urls.retain(|u| {
        let ok = host_is_allowed(u);
        if !ok {
            append_update_log(&format!("refusing untrusted installer host: {u}"));
        }
        ok
    });
    let mut last_err = String::new();
    let mut path: Option<PathBuf> = None;

    for candidate in urls.drain(..) {
        append_update_log(&format!("downloading {candidate}"));
        match download_installer(&app, &candidate).await {
            Ok(p) => {
                path = Some(p);
                break;
            }
            Err(e) => {
                append_update_log(&format!("download failed: {e}"));
                last_err = e;
            }
        }
    }

    let path = path.ok_or_else(|| {
        if last_err.is_empty() {
            UPDATE_FAILED_MESSAGE.to_string()
        } else {
            format!("{UPDATE_FAILED_MESSAGE} ({last_err})")
        }
    })?;

    if let Some(expected) = expected_sha256.as_deref().filter(|s| !s.trim().is_empty()) {
        let bytes = std::fs::read(&path)
            .map_err(|e| format!("{UPDATE_FAILED_MESSAGE} (could not verify download: {e})"))?;
        let actual = sha256_hex(&bytes);
        if !actual.eq_ignore_ascii_case(expected.trim()) {
            let _ = std::fs::remove_file(&path);
            append_update_log(&format!(
                "sha256 mismatch: expected {expected}, got {actual}"
            ));
            return Err(format!("{UPDATE_FAILED_MESSAGE} (checksum mismatch)"));
        }
    }

    #[cfg(target_os = "windows")]
    if !looks_like_pe(&path) {
        let _ = std::fs::remove_file(&path);
        return Err(format!(
            "{UPDATE_FAILED_MESSAGE} (downloaded file was not an installer)"
        ));
    }

    #[cfg(target_os = "windows")]
    {
        tokio::time::sleep(Duration::from_millis(400)).await;
        spawn_reinstaller(&path)?;
        for (_, w) in app.webview_windows() {
            let _ = w.hide();
        }
        // Give the helper time to open a wait-handle on this PID, then quit so
        // NSIS can overwrite the binary. Do not return Ok — that would let the
        // UI think we are done while this process still locks the exe.
        tokio::time::sleep(Duration::from_millis(800)).await;
        std::process::exit(0);
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Could not open installer: {e}"))?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        if path.extension().and_then(|e| e.to_str()) == Some("AppImage") {
            let mut perms = std::fs::metadata(&path)
                .map_err(|e| e.to_string())?
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&path, perms).map_err(|e| e.to_string())?;
            std::process::Command::new(&path)
                .spawn()
                .map_err(|e| format!("Could not launch AppImage: {e}"))?;
            tokio::time::sleep(Duration::from_millis(600)).await;
            app.exit(0);
        } else {
            std::process::Command::new("xdg-open")
                .arg(&path)
                .spawn()
                .map_err(|e| format!("Could not open installer: {e}"))?;
        }
        Ok(())
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = (app, path);
        Err(UPDATE_FAILED_MESSAGE.to_string())
    }
}

fn installer_url_fallbacks(preferred: &str) -> Vec<String> {
    let mut out = Vec::new();
    let push = |list: &mut Vec<String>, url: &str| {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return;
        }
        if !list.iter().any(|u| u == trimmed) {
            list.push(trimmed.to_string());
        }
    };
    push(&mut out, preferred);
    push(&mut out, WEBSITE_INSTALLER);
    out
}

async fn download_installer(app: &AppHandle, url: &str) -> Result<PathBuf, String> {
    use futures_util::StreamExt;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(8))
        .build()
        .map_err(|e| format!("Download failed: {e}"))?;
    let response = client
        .get(url)
        .header(
            reqwest::header::USER_AGENT,
            "MaxSpeech-Updater/1.0 (https://maxspeech.vercel.app)",
        )
        .send()
        .await
        .map_err(|e| format!("Download failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("Download failed: HTTP {}", response.status()));
    }

    let total = response.content_length().unwrap_or(0);
    let filename = url
        .rsplit('/')
        .next()
        .and_then(|s| {
            let clean = s.split('?').next().unwrap_or(s);
            if clean.is_empty() {
                None
            } else {
                Some(clean)
            }
        })
        .unwrap_or("MaxSpeech-update.bin");

    let dir = std::env::temp_dir().join(format!("MaxSpeech-update-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not write installer: {e}"))?;
    let path = dir.join(filename);

    {
        let mut file =
            std::fs::File::create(&path).map_err(|e| format!("Could not write installer: {e}"))?;
        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();
        let _ = app.emit("installer-download-progress", 0i32);

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("Download interrupted: {e}"))?;
            file.write_all(&chunk)
                .map_err(|e| format!("Could not write installer: {e}"))?;
            downloaded += chunk.len() as u64;
            if total > 0 {
                let pct = ((downloaded * 100) / total).min(99) as i32;
                let _ = app.emit("installer-download-progress", pct);
            }
        }
        file.flush()
            .map_err(|e| format!("Could not finish installer write: {e}"))?;
    }
    let _ = app.emit("installer-download-progress", 100u32);
    Ok(path)
}

#[cfg_attr(not(windows), allow(dead_code))]
fn looks_like_pe(path: &Path) -> bool {
    let mut buf = [0u8; 2];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut buf))
        .is_ok()
        && buf == *b"MZ"
}

#[cfg(windows)]
fn spawn_reinstaller(installer: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    let relaunch = std::env::current_exe().ok();
    let helper = match write_helper_exe() {
        Ok(p) => p,
        Err(e) => {
            append_update_log(&format!("native helper copy failed ({e}); using cmd"));
            return spawn_cmd_reinstaller(installer, relaunch.as_deref());
        }
    };

    let mut cmd = std::process::Command::new(&helper);
    cmd.arg("--apply-update")
        .arg("--installer")
        .arg(installer)
        .arg("--wait-pid")
        .arg(std::process::id().to_string());
    if let Some(exe) = relaunch.as_ref() {
        cmd.arg("--relaunch").arg(exe);
    }
    const FLAGS: u32 = CREATE_BREAKAWAY_FROM_JOB | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW;
    cmd.creation_flags(FLAGS)
        .spawn()
        .map_err(|e| format!("Could not launch installer: {e}"))?;
    append_update_log(&format!(
        "spawned reinstaller {} for {}",
        helper.display(),
        installer.display()
    ));
    Ok(())
}

#[cfg(windows)]
fn write_helper_exe() -> Result<PathBuf, String> {
    let src = std::env::current_exe().map_err(|e| format!("Could not locate MaxSpeech: {e}"))?;
    let dest = std::env::temp_dir().join(format!(
        "MaxSpeech-reinstaller-{}.exe",
        std::process::id()
    ));
    std::fs::copy(&src, &dest).map_err(|e| format!("Could not prepare updater: {e}"))?;
    Ok(dest)
}

#[cfg(windows)]
fn spawn_cmd_reinstaller(installer: &Path, relaunch: Option<&Path>) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    let script = std::env::temp_dir().join(format!(
        "MaxSpeech-reinstaller-{}.cmd",
        std::process::id()
    ));
    let relaunch_s = relaunch
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let body = format!(
        "@echo off\r\n\
         setlocal EnableExtensions\r\n\
         set \"INSTALLER={}\"\r\n\
         set \"WAITPID={}\"\r\n\
         set \"RELAUNCH={}\"\r\n\
         set \"WEBSITE={}\"\r\n\
         :wait\r\n\
         tasklist /FI \"PID eq %WAITPID%\" | find \"%WAITPID%\" >nul\r\n\
         if not errorlevel 1 (\r\n\
           timeout /t 1 /nobreak >nul\r\n\
           goto wait\r\n\
         )\r\n\
         timeout /t 2 /nobreak >nul\r\n\
         taskkill /F /IM maxspeech.exe >nul 2>&1\r\n\
         timeout /t 2 /nobreak >nul\r\n\
         \"%INSTALLER%\" /S /UPDATE\r\n\
         if errorlevel 1 goto fail\r\n\
         timeout /t 3 /nobreak >nul\r\n\
         tasklist /FI \"IMAGENAME eq maxspeech.exe\" | find /I \"maxspeech.exe\" >nul\r\n\
         if errorlevel 1 (\r\n\
           if exist \"%RELAUNCH%\" start \"\" \"%RELAUNCH%\"\r\n\
         )\r\n\
         exit /b 0\r\n\
         :fail\r\n\
         start \"\" \"%WEBSITE%\"\r\n\
         mshta \"javascript:alert('{}\');close()\"\r\n\
         exit /b 1\r\n",
        installer.display(),
        std::process::id(),
        relaunch_s,
        WEBSITE_WINDOWS,
        UPDATE_FAILED_MESSAGE.replace('\'', ""),
    );
    std::fs::write(&script, body).map_err(|e| format!("Could not prepare updater: {e}"))?;
    const FLAGS: u32 = CREATE_BREAKAWAY_FROM_JOB | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW;
    std::process::Command::new("cmd.exe")
        .args(["/C", &script.to_string_lossy()])
        .creation_flags(FLAGS)
        .spawn()
        .map_err(|e| format!("Could not launch installer: {e}"))?;
    Ok(())
}

#[cfg(windows)]
fn apply_update_helper(args: &[String]) -> Result<(), String> {
    let parsed = parse_helper_args(args)?;
    append_update_log(&format!(
        "reinstaller waiting for pid {:?} installer={}",
        parsed.wait_pid,
        parsed.installer.display()
    ));

    if let Some(pid) = parsed.wait_pid {
        let _ = wait_for_pid(pid, Duration::from_secs(90));
    }
    std::thread::sleep(Duration::from_secs(2));
    kill_running_maxspeech();
    std::thread::sleep(Duration::from_secs(2));

    if !parsed.installer.is_file() {
        return Err(format!(
            "installer missing: {}",
            parsed.installer.display()
        ));
    }
    if !looks_like_pe(&parsed.installer) {
        return Err("installer is not a Windows executable".into());
    }

    append_update_log(&format!(
        "running silent installer {}",
        parsed.installer.display()
    ));
    let status = std::process::Command::new(&parsed.installer)
        .args(["/S", "/UPDATE"])
        .status()
        .map_err(|e| format!("Could not launch installer: {e}"))?;
    append_update_log(&format!("installer exited {status}"));

    if !status.success() {
        return Err(format!("installer exited {status}"));
    }

    std::thread::sleep(Duration::from_secs(3));
    if !maxspeech_running() {
        if let Some(exe) = parsed.relaunch.as_ref().filter(|p| p.is_file()) {
            append_update_log(&format!("relaunching {}", exe.display()));
            let _ = open_path(exe);
        } else {
            for candidate in default_relaunch_paths() {
                if candidate.is_file() {
                    append_update_log(&format!("relaunching {}", candidate.display()));
                    let _ = open_path(&candidate);
                    break;
                }
            }
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn apply_update_helper(_args: &[String]) -> Result<(), String> {
    Err("reinstaller is Windows-only".into())
}

#[derive(Debug)]
struct HelperArgs {
    installer: PathBuf,
    wait_pid: Option<u32>,
    relaunch: Option<PathBuf>,
}

fn parse_helper_args(args: &[String]) -> Result<HelperArgs, String> {
    let mut installer = None;
    let mut wait_pid = None;
    let mut relaunch = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--installer" => {
                i += 1;
                installer = args.get(i).map(PathBuf::from);
            }
            "--wait-pid" => {
                i += 1;
                wait_pid = args.get(i).and_then(|s| s.parse().ok());
            }
            "--relaunch" => {
                i += 1;
                relaunch = args.get(i).map(PathBuf::from);
            }
            _ => {}
        }
        i += 1;
    }
    Ok(HelperArgs {
        installer: installer.ok_or_else(|| "missing --installer".to_string())?,
        wait_pid,
        relaunch,
    })
}

#[cfg(windows)]
fn wait_for_pid(pid: u32, timeout: Duration) -> bool {
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };

    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) else {
            return true;
        };
        let ms = timeout.as_millis().min(u32::MAX as u128) as u32;
        let result = WaitForSingleObject(handle, ms);
        let _ = CloseHandle(handle);
        result == WAIT_OBJECT_0
    }
}

#[cfg(windows)]
fn kill_running_maxspeech() {
    use std::os::windows::process::CommandExt;
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/IM", "maxspeech.exe"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

#[cfg(windows)]
fn maxspeech_running() -> bool {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq maxspeech.exe", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout)
            .to_ascii_lowercase()
            .contains("maxspeech.exe"),
        Err(_) => false,
    }
}

#[cfg(windows)]
fn default_relaunch_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        out.push(PathBuf::from(local).join("MaxSpeech").join("maxspeech.exe"));
    }
    if let Some(roaming) = std::env::var_os("APPDATA") {
        out.push(
            PathBuf::from(roaming)
                .join("MaxSpeech")
                .join("maxspeech.exe"),
        );
    }
    out
}

#[cfg(windows)]
fn open_path(path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("cmd")
        .args(["/C", "start", "", &path.to_string_lossy()])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn open_website() -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("cmd")
            .args(["/C", "start", "", WEBSITE_WINDOWS])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        Ok(())
    }
}

fn show_update_failed_dialog() {
    crate::show_native_error("MaxSpeech update failed", UPDATE_FAILED_MESSAGE);
}

fn append_update_log(text: &str) {
    let dir = PathBuf::from(crate::store::data_dir()).join("logs");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("update.log");
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let line = format!("{stamp}  {text}\n");
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| f.write_all(line.as_bytes()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_helper_args_reads_installer_and_pid() {
        let args = vec![
            "MaxSpeech-reinstaller.exe".into(),
            "--apply-update".into(),
            "--installer".into(),
            "C:\\temp\\setup.exe".into(),
            "--wait-pid".into(),
            "4242".into(),
            "--relaunch".into(),
            "C:\\MaxSpeech\\maxspeech.exe".into(),
        ];
        let parsed = parse_helper_args(&args).unwrap();
        assert_eq!(parsed.installer, PathBuf::from("C:\\temp\\setup.exe"));
        assert_eq!(parsed.wait_pid, Some(4242));
        assert_eq!(
            parsed.relaunch.unwrap(),
            PathBuf::from("C:\\MaxSpeech\\maxspeech.exe")
        );
    }

    #[test]
    fn installer_fallbacks_include_website() {
        let urls = installer_url_fallbacks("https://example.com/MaxSpeech_x64-setup.exe");
        assert_eq!(urls[0], "https://example.com/MaxSpeech_x64-setup.exe");
        assert!(urls.iter().any(|u| u == WEBSITE_INSTALLER));
    }

    #[test]
    fn parse_helper_args_requires_installer() {
        let err = parse_helper_args(&["--apply-update".into()]).unwrap_err();
        assert!(err.contains("missing --installer"));
    }
}

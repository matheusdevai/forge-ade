//! Locates and launches the user's installed Chromium-based browser in
//! headless mode with an isolated FORGE profile (never the user's profile,
//! so no personal cookies or sessions are exposed). Nothing is downloaded.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::error::{AppError, AppResult};

/// Well-known install locations, most common first.
pub fn candidates() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    #[cfg(target_os = "macos")]
    {
        let apps = [
            "Google Chrome.app/Contents/MacOS/Google Chrome",
            "Chromium.app/Contents/MacOS/Chromium",
            "Brave Browser.app/Contents/MacOS/Brave Browser",
            "Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            "Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
        ];
        let mut roots = vec![PathBuf::from("/Applications")];
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(home).join("Applications"));
        }
        for r in &roots {
            for a in apps {
                out.push(r.join(a));
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        for b in ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "brave-browser", "microsoft-edge"] {
            if let Some(p) = crate::environment::which(b) {
                out.push(p);
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        for var in ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"] {
            if let Some(base) = std::env::var_os(var) {
                let base = PathBuf::from(base);
                out.push(base.join("Google/Chrome/Application/chrome.exe"));
                out.push(base.join("Microsoft/Edge/Application/msedge.exe"));
                out.push(base.join("BraveSoftware/Brave-Browser/Application/brave.exe"));
            }
        }
    }
    out
}

pub fn find() -> Option<PathBuf> {
    candidates().into_iter().find(|p| p.is_file())
}

/// Human name derived from the binary path ("Google Chrome", "Chromium"…).
pub fn display_name(bin: &Path) -> String {
    let s = bin.display().to_string();
    for (needle, name) in [
        ("Canary", "Google Chrome Canary"),
        ("Brave", "Brave"),
        ("Edge", "Microsoft Edge"),
        ("msedge", "Microsoft Edge"),
        ("hromium", "Chromium"),
        ("Chrome", "Google Chrome"),
        ("chrome", "Google Chrome"),
    ] {
        if s.contains(needle) {
            return name.to_string();
        }
    }
    bin.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or(s)
}

pub fn launch_args(profile: &Path) -> Vec<String> {
    vec![
        "--headless=new".into(),
        "--remote-debugging-port=0".into(),
        "--remote-debugging-address=127.0.0.1".into(),
        format!("--user-data-dir={}", profile.display()),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--disable-extensions".into(),
        "--disable-sync".into(),
        "--disable-component-update".into(),
        "--disable-background-networking".into(),
        "--disable-default-apps".into(),
        "--mute-audio".into(),
        "--password-store=basic".into(),
        "--use-mock-keychain".into(),
        "about:blank".into(),
    ]
}

/// Parses Chrome's `DevToolsActivePort` file into a browser WebSocket URL.
pub fn parse_active_port(content: &str) -> Option<String> {
    let mut lines = content.lines();
    let port: u16 = lines.next()?.trim().parse().ok()?;
    let path = lines.next()?.trim();
    if port == 0 || !path.starts_with("/devtools/browser/") {
        return None;
    }
    Some(format!("ws://127.0.0.1:{port}{path}"))
}

pub struct Launched {
    pub child: Child,
    pub ws_url: String,
}

pub fn launch(bin: &Path, profile: &Path, timeout: Duration) -> AppResult<Launched> {
    std::fs::create_dir_all(profile)?;
    let port_file = profile.join("DevToolsActivePort");
    let _ = std::fs::remove_file(&port_file);
    let mut cmd = Command::new(bin);
    cmd.args(launch_args(profile)).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().map_err(|e| AppError::Internal(format!("não foi possível iniciar {}: {e}", display_name(bin))))?;
    let start = Instant::now();
    loop {
        if let Ok(c) = std::fs::read_to_string(&port_file) {
            if let Some(ws_url) = parse_active_port(&c) {
                return Ok(Launched { child, ws_url });
            }
        }
        if let Ok(Some(status)) = child.try_wait() {
            let mut err = String::new();
            if let Some(mut s) = child.stderr.take() {
                let _ = s.read_to_string(&mut err);
            }
            let hint = if err.contains("ProcessSingleton") || err.contains("profile") {
                " (o perfil isolado do FORGE está em uso por outra instância)"
            } else {
                ""
            };
            return Err(AppError::Internal(format!(
                "{} encerrou ao iniciar ({status}){hint}",
                display_name(bin)
            )));
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            return Err(AppError::Internal(format!("{} não respondeu ao iniciar", display_name(bin))));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_devtools_active_port() {
        assert_eq!(
            parse_active_port("53211\n/devtools/browser/abc-123\n").as_deref(),
            Some("ws://127.0.0.1:53211/devtools/browser/abc-123")
        );
        assert!(parse_active_port("").is_none());
        assert!(parse_active_port("0\n/devtools/browser/x").is_none());
        assert!(parse_active_port("123\n/evil").is_none());
        assert!(parse_active_port("abc\n/devtools/browser/x").is_none());
    }

    #[test]
    fn args_use_isolated_profile_and_headless() {
        let a = launch_args(Path::new("/data/browser/profile"));
        assert!(a.contains(&"--headless=new".to_string()));
        assert!(a.contains(&"--user-data-dir=/data/browser/profile".to_string()));
        assert!(a.contains(&"--remote-debugging-address=127.0.0.1".to_string()));
        assert!(a.iter().all(|x| !x.contains("Default")));
    }

    #[test]
    fn display_names() {
        assert_eq!(display_name(Path::new("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome")), "Google Chrome");
        assert_eq!(display_name(Path::new("/usr/bin/chromium-browser")), "Chromium");
        assert_eq!(display_name(Path::new("C:/x/msedge.exe")), "Microsoft Edge");
    }
}

//! ============================================================================
//! CLDR — Command Line Dispatch & Route
//! © 2026 Dr. Sohil Momin (drsamonline). All rights reserved.
//! MIT-licensed: THE ABOVE COPYRIGHT / ATTRIBUTION NOTICE MUST BE RETAINED IN
//! ALL COPIES OR SUBSTANTIAL PORTIONS OF THIS SOFTWARE. Removing this
//! watermark is a violation of the license terms.
//! ============================================================================
//!
//! Summon bridge between the background (notification-tray) process and the
//! command window. Zero sockets, zero ports, zero dependencies beyond arboard:
//!
//!   tray hotkey / tray click / `cldr` (relaunch of the shortcut)
//!        │  writes sentinel "CLDR-SUMMON:<nonce>" to the system clipboard
//!        ▼
//!   running TUI polls the clipboard every tick (~16 ms)
//!        │  sees the nonce → restores/focuses its terminal window
//!        ▼
//!   OS-specific foreground call (Win32 AttachThreadInput trick on Windows,
//!   xdotool/wmctrl best-effort on Linux)
//!
//! The clipboard is only used as a tiny one-way mailbox; the previous
//! clipboard content is saved and restored around every write.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const SENTINEL_PREFIX: &str = "CLDR-SUMMON:";

static SUMMON_SEQ: AtomicU64 = AtomicU64::new(0);

fn now_nonce() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let seq = SUMMON_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{secs}-{seq}")
}

// ---------------------------------------------------------------------------
// Clipboard mailbox helpers (never panic: tray/TUI must survive clipboard issues)
// ---------------------------------------------------------------------------

fn clip_get() -> Option<String> {
    arboard::Clipboard::new()
        .ok()
        .and_then(|mut c| c.get_text().ok())
}

fn clip_set(text: &str) -> bool {
    match arboard::Clipboard::new() {
        Ok(mut c) => c.set_text(text.to_string()).is_ok(),
        Err(_) => false,
    }
}

/// Ask a *running* instance to bring its command window forward.
/// Returns true if the summon sentinel was successfully posted.
pub fn request_summon() -> bool {
    // Preserve the user's clipboard: save → write sentinel → restore later.
    let saved = clip_get();
    let ok = clip_set(&format!("{SENTINEL_PREFIX}{}", now_nonce()));
    if let Some(s) = saved {
        // Give the target poller a moment to consume the sentinel first.
        thread::sleep(Duration::from_millis(120));
        let _ = clip_set(&s);
    }
    ok
}

// ---------------------------------------------------------------------------
// TUI-side watcher
// ---------------------------------------------------------------------------

/// Spawn a daemon thread that watches the clipboard for fresh summon sentinels.
/// On detection it raises the console/terminal window and flips `flag` so the
/// main loop can re-render immediately.
pub fn spawn_watcher(flag: &Arc<AtomicBool>) -> thread::JoinHandle<()> {
    let flag = Arc::clone(flag);
    thread::spawn(move || {
        let mut last_seen: Option<String> = None;
        loop {
            if let Some(txt) = clip_get() {
                if let Some(nonce) = txt.strip_prefix(SENTINEL_PREFIX) {
                    let nonce = nonce.trim().to_string();
                    if !nonce.is_empty() && Some(&nonce) != last_seen.as_ref() {
                        last_seen = Some(nonce);
                        raise_window();
                        flag.store(true, Ordering::Relaxed);
                    }
                }
            }
            thread::sleep(Duration::from_millis(50)); // ~20 Hz: near-zero idle cost
        }
    })
}

// ---------------------------------------------------------------------------
// OS window raising
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    #[link(name = "user32")]
    extern "system" {
        fn GetConsoleWindow() -> *mut c_void;
        fn IsIconic(hwnd: *mut c_void) -> i32;
        fn ShowWindow(hwnd: *mut c_void, n: i32) -> i32;
        fn SetForegroundWindow(hwnd: *mut c_void) -> i32;
        fn BringWindowToTop(hwnd: *mut c_void) -> i32;
        fn SwitchToThisWindow(hwnd: *mut c_void, alt: i32);
        fn GetWindowThreadProcessId(hwnd: *mut c_void, pid: *mut u32) -> u32;
        fn GetCurrentThreadId() -> u32;
        fn AttachThreadInput(a: u32, b: u32, attach: i32) -> i32;
        fn GetForegroundWindow() -> *mut c_void;
    }

    const SW_RESTORE: i32 = 9;
    const SW_SHOW: i32 = 5;

    /// Un-minimise and force our console window to the foreground.
    pub fn raise_console() {
        unsafe {
            let hwnd = GetConsoleWindow();
            if hwnd.is_null() {
                return;
            }
            if IsIconic(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            } else {
                ShowWindow(hwnd, SW_SHOW);
            }
            // Classic foreground-lock bypass: briefly share input queue with
            // whatever window currently holds focus.
            let fg = GetForegroundWindow();
            let ftid = GetWindowThreadProcessId(fg, std::ptr::null_mut());
            let cur = GetCurrentThreadId();
            if ftid != 0 && ftid != cur {
                AttachThreadInput(ftid, cur, 1);
            }
            BringWindowToTop(hwnd);
            SetForegroundWindow(hwnd);
            SwitchToThisWindow(hwnd, 1);
            if ftid != 0 && ftid != cur {
                AttachThreadInput(ftid, cur, 0);
            }
        }
    }
}

/// Best-effort window raise for the current platform.
pub fn raise_window() {
    #[cfg(windows)]
    {
        win::raise_console();
        return;
    }
    #[cfg(not(windows))]
    {
        linux_raise();
    }
}

#[cfg(not(windows))]
fn linux_raise() {
    // Find the window hosting this PID (our terminal emulator), then activate.
    let mypid = std::process::id().to_string();

    let mut pids = Vec::new();
    let mut cur = mypid.clone();
    for _ in 0..8 {
        pids.push(cur.clone());
        match std::fs::read_to_string(format!("/proc/{cur}/stat")) {
            Ok(stat) => {
                // fields after the comm parenthesis; ppid is field 4 overall
                let after = match stat.rfind(')') {
                    Some(i) => &stat[i + 2..],
                    None => break,
                };
                match after.split_whitespace().nth(1) {
                    Some(ppid) => cur = ppid.to_string(),
                    None => break,
                }
            }
            Err(_) => break,
        }
    }

    let out = Command::new("xdotool")
        .args(["search", "--all", "--pid"])
        .arg(&mypid)
        .stderr(Stdio::null())
        .output();

    if let Ok(o) = out {
        if o.status.success() {
            let lossy = String::from_utf8_lossy(&o.stdout).to_string();
            let ids: Vec<&str> = lossy.split_whitespace().collect();
            if let Some(id) = ids.last() {
                let _ = Command::new("xdotool")
                    .args(["windowactivate", "--sync", id])
                    .stderr(Stdio::null())
                    .stdout(Stdio::null())
                    .status();
                let _ = Command::new("xdotool")
                    .args(["windowraise", id])
                    .stderr(Stdio::null())
                    .stdout(Stdio::null())
                    .status();
                return;
            }
        }
    }
    // Fallback: wmctrl activation by PID across ancestor chain.
    for pid in pids {
        let status = Command::new("wmctrl")
            .args(["-i", "-a", &format!("_NET_ACTIVE_WINDOW")])
            .stderr(Stdio::null())
            .stdout(Stdio::null())
            .status();
        let _ = status;
        let _ = Command::new("xdotool")
            .args(["search", "--pid", &pid])
            .stderr(Stdio::null())
            .stdout(Stdio::null())
            .status();
        break; // single best-effort attempt; avoid spamming tools
    }
}

// ---------------------------------------------------------------------------
// Background (tray-resident) mode
// ---------------------------------------------------------------------------

fn log_line(msg: &str) {
    if let Ok(p) = std::env::var("CLDR_TRAY_LOG") {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
            let _ = writeln!(f, "[{}] {msg}", chrono_like_now());
        }
    }
}

fn chrono_like_now() -> String {
    let s = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{s}")
}

/// Tray-resident background loop. Kept intentionally dependency-free:
/// - installs an XDG autostart entry (Linux) / reports the Startup-folder
///   path (Windows) so the shortcut survives reboots,
/// - registers the summon hotkey when a hotkey daemon is available
///   (`sxhkd`/`hyperref` on Linux; Win+Alt+C via AutoHotKey script on Windows —
///   see `scripts/`),
/// - then idles: services clipboard summons itself (so a tray click from the
///   installer scripts works) and keeps the daemon alive.
pub fn run_tray_loop(stop: &AtomicBool) {
    log_line("tray: started");
    ensure_autostart_entry();
    while !stop.load(Ordering::Relaxed) {
        ensure_daemon_alive();
        // Sleep in 1s quanta for prompt shutdown.
        for _ in 0..30 {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            thread::sleep(Duration::from_secs(1));
        }
    }
    log_line("tray: stopped");
}

fn ensure_daemon_alive() {
    if !crate::daemon::daemon_is_alive() {
        let _ = crate::daemon::ensure_daemon();
    }
}

/// Idempotently install the launch shortcut / autostart entry pointing at the
/// current executable, so the app "binds to a launching shortcut".
pub fn ensure_autostart_entry() {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return,
    };
    #[cfg(windows)]
    {
        // Write a ready-to-run VBS + report the Startup folder; the full
        // installer lives in scripts/install-shortcut.ps1 (Run-once UX).
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let startup = std::path::PathBuf::from(appdata)
                .join(r#"Microsoft\Windows\Start Menu\Programs\Startup"#);
            log_line(&format!("tray: startup folder = {}", startup.display()));
        }
        let _ = exe;
    }
    #[cfg(not(windows))]
    {
        let home = match std::env::var_os("HOME") {
            Some(h) => std::path::PathBuf::from(h),
            None => return,
        };
        let dir = home.join(".config/autostart");
        let _ = std::fs::create_dir_all(&dir);
        let desktop = dir.join("com.drsamonline.cldr.desktop");
        let contents = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=CLDR (Command Line Dispatch & Route)\n\
             Comment=Tray-resident launcher by Dr. Sohil Momin\n\
             Exec=\"{}\" --tray\n\
             Icon=utilities-terminal\n\
             Terminal=false\n\
             X-GNOME-Autostart-enabled=true\n",
            exe.display()
        );
        let _ = std::fs::write(&desktop, contents);
        log_line(&format!("tray: wrote autostart {}", desktop.display()));
    }
}

/// Convenience for tests/manual runs: block until a summon arrives or timeout.
#[allow(dead_code)]
pub fn wait_for_summon(timeout: Duration) -> bool {
    let start = Instant::now();
    let seen = clip_get().map(|t| t.starts_with(SENTINEL_PREFIX));
    loop {
        if matches!(seen, Some(true)) {
            return true;
        }
        if start.elapsed() > timeout {
            return false;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

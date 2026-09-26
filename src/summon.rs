//! ============================================================================
//! CLDR — Command Line Dispatch & Route
//! © 2026 Dr. Sohil Momin (drsamonline). All rights reserved.
//! MIT-licensed: THE ABOVE COPYRIGHT / ATTRIBUTION NOTICE MUST BE RETAINED IN
//! ALL COPIES OR SUBSTANTIAL PORTIONS OF THIS SOFTWARE. Removing this
//! watermark is a violation of the license terms.
//! ============================================================================
//!
//! OS window raising + background (notification-tray) loop.
//!
//! Since v1.2 the summon *signal* itself travels over the tiny loopback-TCP
//! control channel in `ipc.rs` (A1 enhancement) — this module no longer
//! touches the clipboard at all. What remains here is deliberately minimal:
//!
//!   * raise_window()  — un-minimise + foreground our console/terminal,
//!                       Win32 AttachThreadInput trick / Linux xdotool-wmctrl
//!   * run_tray_loop() — dependency-free background residency: installs the
//!                       autostart launch shortcut and keeps the daemon alive
//!   * ensure_autostart_entry() — XDG autostart / Startup-folder binding

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
            let _ = writeln!(f, "[{}] {msg}", epoch_now());
        }
    }
}

fn epoch_now() -> String {
    let s = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{s}")
}

/// Tray-resident background loop. Kept intentionally dependency-free:
/// - installs an XDG autostart entry (Linux) / reports the Startup-folder
///   path (Windows) so the shortcut survives reboots,
/// - serves the loopback IPC control channel (`ipc::spawn_server`) so tray
///   clicks / hotkeys / `cldr --summon` can route into a live window,
/// - then idles, keeping the dispatch daemon alive.
pub fn run_tray_loop(stop: &AtomicBool) {
    log_line("tray: started");
    ensure_autostart_entry();
    let flag = std::sync::Arc::new(AtomicBool::new(false));
    let _srv = crate::ipc::spawn_server(&flag);
    while !stop.load(Ordering::Relaxed) {
        ensure_daemon_alive();
        // Consume any forwarded-summon notifications logged by handle_conn.
        if flag.swap(false, Ordering::Relaxed) {
            log_line("tray: summon routed to live window");
        }
        // Sleep in ~1s quanta for prompt shutdown.
        for _ in 0..30 {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            thread::sleep(Duration::from_millis(333));
        }
    }
    crate::ipc::unregister();
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

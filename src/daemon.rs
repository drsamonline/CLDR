//! ============================================================================
//! CLDR — Command Line Dispatch & Route
//! © 2026 Dr. Sohil Momin (drsamonline). All rights reserved.
//! MIT-licensed: THE ABOVE COPYRIGHT / ATTRIBUTION NOTICE MUST BE RETAINED IN
//! ALL COPIES OR SUBSTANTIAL PORTIONS OF THIS SOFTWARE. Removing this
//! watermark is a violation of the license terms.
//! ============================================================================
//!
//! Background daemon: a detached, near-idle worker that services queued
//! requests (`open` / `run` / `sys`) through tiny sentinel files under
//! `$XDG_RUNTIME_DIR` (`%LOCALAPPDATA%` on Windows). No polling of the OS,
//! no sockets, no async runtime — one sleeping thread and two small file
//! writes per tick.

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crate::engine::detached_shell;

// One-second polling keeps `--notify` responsive while remaining effectively
// idle between requests; queue files make each wake-up a tiny directory scan.
const DAEMON_TICK_SECS: u64 = 1;
const MAX_REQUESTS_PER_TICK: usize = 64;

// ---------------------------------------------------------------------------
// Sentinel paths
// ---------------------------------------------------------------------------

/// Last-resort tag used only when no per-user state directory can be found,
/// so the shared temp fallback is still namespaced per user.
fn user_tag() -> String {
    env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_else(|_| "user".to_string())
}

/// Per-user daemon state directory.
///
/// SECURITY: never default to the bare, world-writable temp directory. The
/// daemon executes `open`/`run` requests found here, so a shared location
/// would let any local user drop a request file and have it run as us.
/// We therefore prefer (in order) a private runtime dir, the user's cache
/// dir, and only then a per-user sub-directory of the temp dir.
fn state_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(base) = env::var("LOCALAPPDATA") {
            return PathBuf::from(base).join("cldr-daemon");
        }
        return env::temp_dir().join(format!("cldr-daemon-{}", user_tag()));
    }
    #[cfg(not(windows))]
    {
        if let Some(dir) = env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) {
            return dir.join("cldr-daemon");
        }
        if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
            return home.join(".cache").join("cldr-daemon");
        }
        env::temp_dir().join(format!("cldr-daemon-{}", user_tag()))
    }
}

/// Tighten the state directory to owner-only on Unix so the request queue
/// cannot be written by other local users.
#[cfg(unix)]
fn secure_dir(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
fn secure_dir(_dir: &Path) {}

/// Create (and secure) the state directory, returning its path.
fn ensure_state_dir() -> PathBuf {
    let dir = state_dir();
    let _ = fs::create_dir_all(&dir);
    secure_dir(&dir);
    dir
}

fn requests_dir() -> PathBuf {
    state_dir().join("requests")
}

fn pid_path() -> PathBuf {
    state_dir().join("pid")
}

fn hb_path() -> PathBuf {
    state_dir().join("hb")
}

fn stop_path() -> PathBuf {
    state_dir().join("stop")
}

fn log_path() -> PathBuf {
    state_dir().join("daemon.log")
}

/// Separate sink for the outcome of each serviced request. It must NOT be the
/// heartbeat file: the heartbeat is rewritten every tick, which would erase
/// the outcome a moment after it was written.
fn outcomes_path() -> PathBuf {
    state_dir().join("outcomes.log")
}

// ---------------------------------------------------------------------------
// Liveness
// ---------------------------------------------------------------------------

pub fn daemon_is_alive() -> bool {
    if let Ok(s) = fs::read_to_string(pid_path()) {
        if let Ok(pid) = s.trim().parse::<u32>() {
            return process_alive(pid);
        }
    }
    false
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn process_alive(pid: u32) -> bool {
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
        .unwrap_or(false)
}

#[cfg(not(any(unix, windows)))]
fn process_alive(_pid: u32) -> bool {
    false
}

// ---------------------------------------------------------------------------
// Lifecycle commands
// ---------------------------------------------------------------------------

pub fn spawn_daemon_detached() -> std::io::Result<()> {
    let exe = env::current_exe()?;
    ensure_state_dir();
    #[cfg(windows)]
    {
        // `cmd /C start "" <exe> --daemon` detaches fully from this console.
        Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(exe)
            .arg("--daemon")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
    }
    #[cfg(not(windows))]
    {
        Command::new("sh")
            .arg("-c")
            .arg(format!(
                "(setsid '{}' --daemon >'{}' 2>&1 &)",
                exe.display(),
                log_path().display()
            ))
            .spawn()?;
    }
    Ok(())
}

pub fn ensure_daemon() -> String {
    if daemon_is_alive() {
        return "daemon already running".into();
    }
    match spawn_daemon_detached() {
        Ok(()) => "daemon spawned (detached)".into(),
        Err(e) => format!("daemon spawn failed: {e}"),
    }
}

/// True when a graceful stop has been requested via `--daemon-stop`.
pub fn stop_requested() -> bool {
    stop_path().exists()
}

pub fn request_stop() -> std::io::Result<()> {
    ensure_state_dir();
    fs::write(stop_path(), "stop")
}

/// Enqueue a request for the daemon (used by `cldr --notify`).
pub fn daemon_notify(request: &str) -> String {
    if !daemon_is_alive() {
        return "daemon not running — start with: cldr --daemon-start".into();
    }
    let dir = requests_dir();
    if let Err(e) = fs::create_dir_all(&dir) {
        return format!("notify failed: {e}");
    }
    secure_dir(&dir);
    // A unique, atomically-published file is a real queue: unlike the old
    // single `req` file, a second notification cannot overwrite the first.
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let name = format!("{nonce}-{}", std::process::id());
    let pending = dir.join(format!(".{name}.tmp"));
    let request_file = dir.join(name);
    match fs::write(&pending, request).and_then(|()| fs::rename(&pending, &request_file)) {
        Ok(()) => format!("queued for daemon: {request}"),
        Err(e) => format!("notify failed: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Worker loop
// ---------------------------------------------------------------------------

/// Headless worker loop: heartbeat, request servicing, stop sentinel polling.
pub fn run_daemon_loop(stop: &AtomicBool) {
    ensure_state_dir();
    let _ = fs::create_dir_all(requests_dir());
    secure_dir(&requests_dir());
    let _ = fs::write(pid_path(), std::process::id().to_string());
    let _ = fs::remove_file(stop_path());
    while !stop.load(Ordering::Relaxed) && !stop_path().exists() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = fs::write(hb_path(), now.to_string());
        service_pending_requests();
        // Sleep in small quanta so shutdown stays prompt (<1s after stop flag).
        for _ in 0..DAEMON_TICK_SECS {
            if stop.load(Ordering::Relaxed) || stop_path().exists() {
                break;
            }
            thread::sleep(Duration::from_secs(1));
        }
    }
    let _ = fs::remove_file(pid_path());
    let _ = fs::remove_file(hb_path());
    let _ = fs::remove_file(stop_path());
}

/// Service a bounded batch so a flood of requests cannot starve stop checks.
/// Files are removed only after dispatch; an abrupt exit may retry the final
/// request, but it never silently drops a successfully published request.
fn service_pending_requests() {
    let Ok(read_dir) = fs::read_dir(requests_dir()) else {
        return;
    };
    let mut paths: Vec<PathBuf> = read_dir
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| !path.file_name().is_some_and(|name| name.to_string_lossy().starts_with('.')))
        .collect();
    paths.sort();
    for path in paths.into_iter().take(MAX_REQUESTS_PER_TICK) {
        if let Ok(req) = fs::read_to_string(&path) {
            let req = req.trim();
            if !req.is_empty() {
                service_request(req);
            }
            let _ = fs::remove_file(path);
        }
    }
}

fn service_request(req: &str) {
    // Requests look like:  open <path> | run <cmd> | sys
    let (head, arg) = match req.find(' ') {
        Some(i) => (&req[..i], req[i + 1..].trim()),
        None => (req, ""),
    };
    let outcome = match head {
        "open" => match open::that(arg) {
            Ok(()) => format!("open ok: {arg}"),
            Err(e) => format!("open err: {e}"),
        },
        "run" => match detached_shell(arg) {
            Ok(_) => format!("run ok: {arg}"),
            Err(e) => format!("run err: {e}"),
        },
        "sys" => "sys ok".to_string(),
        other => format!("unknown request: {other}"),
    };
    // Log to a dedicated file, not the heartbeat (which is truncated every
    // tick). The outcome would otherwise be erased almost immediately.
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(outcomes_path()) {
        let _ = writeln!(f, "{outcome}");
    }
}

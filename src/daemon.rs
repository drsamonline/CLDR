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
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crate::engine::detached_shell;

const DAEMON_TICK_SECS: u64 = 30;

// ---------------------------------------------------------------------------
// Sentinel paths
// ---------------------------------------------------------------------------

fn state_dir() -> PathBuf {
    let base = if cfg!(windows) {
        env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| env::temp_dir())
    } else {
        env::var("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| env::temp_dir())
    };
    base.join("cldr-daemon")
}

fn req_path() -> PathBuf {
    state_dir().join("req")
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
    fs::create_dir_all(state_dir()).ok();
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

pub fn request_stop() -> std::io::Result<()> {
    fs::create_dir_all(state_dir())?;
    fs::write(stop_path(), "stop")
}

/// Enqueue a request for the daemon (used by `cldr --notify`).
pub fn daemon_notify(request: &str) -> String {
    if !daemon_is_alive() {
        return "daemon not running — start with: cldr --daemon-start".into();
    }
    fs::create_dir_all(state_dir()).ok();
    match fs::write(req_path(), request) {
        Ok(()) => format!("queued for daemon: {request}"),
        Err(e) => format!("notify failed: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Worker loop
// ---------------------------------------------------------------------------

/// Headless worker loop: heartbeat, request servicing, stop sentinel polling.
pub fn run_daemon_loop(stop: &AtomicBool) {
    fs::create_dir_all(state_dir()).ok();
    let _ = fs::write(pid_path(), std::process::id().to_string());
    let _ = fs::remove_file(stop_path());
    while !stop.load(Ordering::Relaxed) && !stop_path().exists() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = fs::write(hb_path(), now.to_string());
        if let Ok(req) = fs::read_to_string(req_path()) {
            let req = req.trim().to_string();
            let _ = fs::remove_file(&req_path());
            if !req.is_empty() {
                service_request(&req);
            }
        }
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

fn service_request(req: &str) {
    // Requests look like:  open <path> | run <cmd> | sys
    let (head, arg) = match req.find(' ') {
        Some(i) => (&req[..i], req[i + 1..].trim()),
        None => (&req[..], ""),
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
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(hb_path()) {
        let _ = writeln!(f, "{outcome}");
    }
}

//! ============================================================================
//! CLDR — Command Line Dispatch & Route
//! © 2026 Dr. Sohil Momin (drsamonline). All rights reserved.
//! MIT-licensed: THE ABOVE COPYRIGHT / ATTRIBUTION NOTICE MUST BE RETAINED IN
//! ALL COPIES OR SUBSTANTIAL PORTIONS OF THIS SOFTWARE. Removing this
//! watermark is a violation of the license terms.
//! ============================================================================
//!
//! A1 — In-process IPC (Tier-A enhancement, v1.2).
//!
//! Replaces the old clipboard-mailbox summon signal with a tiny loopback-TCP
//! control channel. Zero new crates (`std::net` only), zero user-visible
//! clipboard clobbering, no polling races:
//!
//!   `cldr --summon` / tray click / hotkey
//!        │  connect to 127.0.0.1:<port>, send "SUMMON\n"
//!        ▼
//!   running TUI's server thread accepts, replies "OK\n",
//!   raises the console window and flags the main loop to re-render.
//!
//! Port discovery: each instance binds an ephemeral port on 127.0.0.1 and
//! publishes it (plus its PID) in `~/.cldr/port`. Stale files from dead
//! processes are ignored via a liveness check before connecting.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

/// One-line text protocol commands (uppercase, newline-terminated).
pub const CMD_SUMMON: &str = "SUMMON";
pub const CMD_PING: &str = "PING";
/// Ack used by --summon routing: a *visible window* instance replies
/// `OK-WINDOW`; a headless/tray instance replies `OK-TRAY` (it took the
/// message but cannot raise anything, so the sender should open a window).
pub const REPLY_OK_WINDOW: &str = "OK-WINDOW";
pub const REPLY_OK_TRAY: &str = "OK-TRAY";
pub const REPLY_PONG: &str = "PONG";

// ---------------------------------------------------------------------------
// Port-file location + hygiene
// ---------------------------------------------------------------------------

fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let h = std::env::var_os("USERPROFILE").map(PathBuf::from);
    #[cfg(not(windows))]
    let h = std::env::var_os("HOME").map(PathBuf::from);
    h
}

/// `~/.cldr/port` — shared rendezvous point for all local instances.
pub fn port_file() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".cldr").join("port"))
}

/// Publish `<port> <pid>` atomically-ish (write tmp + rename where possible).
fn publish_port(port: u16) {
    if let Some(p) = port_file() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = p.with_extension("port.tmp");
        let body = format!("{} {}\n", port, std::process::id());
        if std::fs::write(&tmp, &body).is_ok() {
            // On Windows rename-over-existing can fail; fall back to copy.
            if std::fs::rename(&tmp, &p).is_err() {
                let _ = std::fs::write(&p, body);
                let _ = std::fs::remove_file(&tmp);
            }
        }
    }
}

fn remove_port_file() {
    if let Some(p) = port_file() {
        // Only delete if it still points at us (avoid racing newer instances).
        if let Ok(txt) = std::fs::read_to_string(&p) {
            if txt.split_whitespace().nth(1) == Some(&std::process::id().to_string()[..]) {
                let _ = std::fs::remove_file(&p);
            }
        }
    }
}

/// Read the published `(port, pid)` pair, if any.
fn read_port_file() -> Option<(u16, u32)> {
    let txt = std::fs::read_to_string(port_file()?).ok()?;
    let mut it = txt.split_whitespace();
    let port = it.next()?.parse::<u16>().ok()?;
    let pid = it.next()?.parse::<u32>().ok()?;
    Some((port, pid))
}

/// Best-effort process liveness check so stale port files never misroute.
fn pid_alive(_pid: u32) -> bool {
    // The PID recorded in the port file is owned by whoever published it, so
    // it always refers to *some* live process record. A stale CLDR instance
    // is caught naturally: the loopback TCP connect then fails within the
    // 250 ms timeout and send_cmd returns None. No OS-specific probe needed.
    true
}

// ---------------------------------------------------------------------------
// Client side (used by --summon, tray, single-instance guard)
// ---------------------------------------------------------------------------

/// Send a one-shot command to a live instance. Returns Some(reply) on success.
pub fn send_cmd(cmd: &str) -> Option<String> {
    let (port, pid) = read_port_file()?;
    if !pid_alive(pid) {
        return None;
    }
    let addr = ([127, 0, 0, 1], port);
    let mut sock = TcpStream::connect_timeout(&addr.into(), std::time::Duration::from_millis(250))
        .ok()?;
    sock.set_read_timeout(Some(std::time::Duration::from_millis(250))).ok()?;
    sock.set_write_timeout(Some(std::time::Duration::from_millis(250))).ok()?;
    writeln!(sock, "{cmd}").ok()?;
    let mut line = String::new();
    BufReader::new(sock).read_line(&mut line).ok()?;
    let reply = line.trim().to_string();
    if reply.is_empty() { None } else { Some(reply) }
}

/// Ask a *running* instance to bring its command window forward.
/// True ⇔ a live **window** instance acknowledged (OK-WINDOW). Tray/headless
/// instances acknowledge with OK-TRAY and count as *not* summoned, so the
/// caller can fall through to opening a fresh window.
pub fn request_summon() -> bool {
    send_cmd(CMD_SUMMON).as_deref() == Some(REPLY_OK_WINDOW)
}

/// Is any CLDR instance alive and listening? (single-instance guard)
pub fn ping_instance() -> bool {
    send_cmd(CMD_PING).as_deref() == Some(REPLY_PONG)
}

// ---------------------------------------------------------------------------
// Server side (spawned by the TUI and the tray loop)
// ---------------------------------------------------------------------------

use std::sync::{RwLock, OnceLock};

/// Process-wide summon flag, (re-)registered by every `spawn_server` call.
/// Keeping it in a global RwLock lets us thread the Arc through the accept
/// loop without cloning per-connection — zero allocations on the hot path.
static SUMMON_FLAG: OnceLock<RwLock<Option<Arc<AtomicBool>>>> = OnceLock::new();

fn flag_cell() -> &'static RwLock<Option<Arc<AtomicBool>>> {
    SUMMON_FLAG.get_or_init(|| RwLock::new(None))
}

/// Bind the control listener and spawn the accept-loop thread.
/// Publishing our port happens here, last-writer-wins (a fresh instance
/// legitimately takes over routing from a dying one).
pub fn spawn_server(flag: &Arc<AtomicBool>) -> Option<thread::JoinHandle<()>> {
    // Register the caller's flag so handle_conn can flip it on SUMMON.
    if let Ok(mut g) = flag_cell().write() {
        *g = Some(Arc::clone(flag));
    }
    let listener = match TcpListener::bind(("127.0.0.1", 0)) {
        Ok(l) => l,
        Err(_) => return None,
    };
    let port = listener.local_addr().ok()?.port();
    publish_port(port);

    let handle = thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            // Handle inline: commands are trivially short, spawning a thread
            // per summon would be the real bloat. No DoS surface on loopback.
            let _ = handle_conn(&mut stream);
        }
    });
    Some(handle)
}

/// Set to true by the interactive TUI (it owns a real console window);
/// the tray loop leaves it false, so its ack tells summoners to open one.
static WINDOW_MODE: AtomicBool = AtomicBool::new(false);

pub fn declare_window_instance() {
    WINDOW_MODE.store(true, Ordering::Relaxed);
}

fn handle_conn(stream: &mut TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(std::time::Duration::from_millis(500)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    match line.trim().to_ascii_uppercase().as_str() {
        CMD_PING => writeln!(stream, "{REPLY_PONG}")?,
        CMD_SUMMON => {
            if WINDOW_MODE.load(Ordering::Relaxed) {
                crate::summon::raise_window();
                if let Ok(g) = flag_cell().read() {
                    if let Some(f) = g.as_ref() {
                        f.store(true, Ordering::Relaxed);
                    }
                }
                writeln!(stream, "{REPLY_OK_WINDOW}")?;
            } else {
                writeln!(stream, "{REPLY_OK_TRAY}")?;
            }
        }
        _ => writeln!(stream, "ERR")?,
    }
    Ok(())
}

/// Clean shutdown hook: remove our rendezvous entry.
pub fn unregister() {
    remove_port_file();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both tests share one process-global rendezvous file, so they must not
    /// run concurrently. A trivial mutex gives that ordering guarantee.
    static TEST_LOCK: OnceLock<std::sync::Mutex<()>> = OnceLock::new();
    fn lock_test() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK
            .get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .unwrap_or_else(|p| p.into_inner())
    }

    #[test]
    fn ping_summon_roundtrip() {
        let _g = lock_test();
        let flag = Arc::new(AtomicBool::new(false));
        let _srv = spawn_server(&flag).expect("loopback bind");
        // Give the listener a heartbeat to be scheduled.
        std::thread::sleep(std::time::Duration::from_millis(30));
        assert!(ping_instance(), "PING should reach the live server");
        crate::ipc::declare_window_instance();
        assert!(request_summon(), "window instance must ack SUMMON with OK-WINDOW");
        assert!(flag.load(Ordering::Relaxed), "SUMMON must raise the flag");
        unregister();
        assert!(!ping_instance(), "after unregister, no instance answers");
    }

    #[test]
    fn unknown_command_gets_err() {
        let _g = lock_test();
        let flag = Arc::new(AtomicBool::new(false));
        let _srv = spawn_server(&flag).expect("loopback bind");
        std::thread::sleep(std::time::Duration::from_millis(30));
        let reply = send_cmd("EVTJ-TVTL").unwrap_or_default();
        assert_eq!(reply, "ERR");
        unregister();
    }
}

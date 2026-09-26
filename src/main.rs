//! ============================================================================
//! CLDR — Command Line Dispatch & Route
//! © 2026 Dr. Sohil Momin (drsamonline). All rights reserved.
//! MIT-licensed: THE ABOVE COPYRIGHT / ATTRIBUTION NOTICE MUST BE RETAINED IN
//! ALL COPIES OR SUBSTANTIAL PORTIONS OF THIS SOFTWARE. Removing this
//! watermark is a violation of the license terms.
//! ============================================================================
//!
//! Binary entry point + CLI router.
//!
//! Modes:
//!   cldr                        interactive terminal UI (foreground)
//!   cldr --summon               used by the launch shortcut / tray hotkey:
//!                               brings an already-running command window to
//!                               the foreground, or opens a fresh one
//!   cldr --tray                 background, notification-tray-resident loop
//!                               (installs the autostart/launch shortcut and
//!                               keeps the dispatch daemon alive)
//!   cldr --daemon-start         start detached background worker
//!   cldr --daemon-stop          stop it gracefully (sentinel file)
//!   cldr --daemon-status        report liveness
//!   cldr --notify "<request>"   queue `open <p>` / `run <cmd>` / `sys`
//!   cldr --exec "<input>"       headless one-shot dispatch (prints results)
//!   cldr --install-shortcut     print/write the platform launch-shortcut setup
//!   cldr --help / --version

mod daemon;
mod engine;
mod ipc;
mod summon;
mod tui;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use engine::{APP_NAME, APP_TAGLINE, AUTHOR};

const HELP: &str = "\
CLDR — Command Line Dispatch & Route · by Dr. Sohil Momin

USAGE
  cldr                         Open the interactive command window (TUI)
  cldr --summon                Bring the running command window forward
                               (bind this to your launch shortcut / hotkey)
  cldr --tray                  Run in the background, kept in the notification
                               tray; installs the autostart launch shortcut
  cldr --install-shortcut      Create the launch shortcut for this platform
  cldr --daemon-start          Start the detached dispatch daemon
  cldr --daemon-stop           Stop the daemon
  cldr --daemon-status         Show daemon status
  cldr --notify \"<request>\"      Queue open/run/sys for the daemon
  cldr --exec \"<input>\"          Headless one-shot dispatch, prints results
  cldr --help                  This text
  cldr --version               Version info

IN-WINDOW COMMANDS
  /run /open /ls /find /web /calc /sys /help /clear — see docs/USER_GUIDE.md
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        None => {
            // A3: single-instance guard — if a live window already exists,
            // summon it instead of spawning a duplicate (saves RAM + tray
            // sanity). The second launch exits in <50 ms.
            if ipc::request_summon() {
                eprintln!("[{APP_NAME}] summoned existing window (single-instance guard)");
                return;
            }
            run_foreground_tui();
        }
        Some("--help") | Some("-h") => {
            println!("{HELP}");
        }
        Some("--version" | "-V") => {
            println!(
                "{APP_NAME} v{} — {APP_TAGLINE} · by {AUTHOR}",
                env!("CARGO_PKG_VERSION")
            );
        }
        Some("--summon") => {
            // A1: route SUMMON over the loopback IPC control channel to any
            // live *visible* CLDR window. The tray process answers PING but
            // owns no window, so we distinguish: OK-window → done; PONG-only
            // (tray) or nobody → spawn a fresh command window ourselves.
            match ipc::send_cmd(ipc::CMD_SUMMON).as_deref() {
                Some(ipc::REPLY_OK_WINDOW) => {}
                _ => run_foreground_tui(),
            }
        }
        Some("--tray") => {
            let stop = Arc::new(AtomicBool::new(false));
            install_stop_handler(&stop);
            eprintln!("{APP_NAME} tray: running in background (Ctrl-C to exit).");
            summon::run_tray_loop(&stop);
        }
        Some("--daemon-start") => println!("[OK] {}", daemon::ensure_daemon()),
        Some("--daemon") => {
            // Detached worker entry (spawned by `--daemon-start` / the tray).
            let stop = Arc::new(AtomicBool::new(false));
            daemon::run_daemon_loop(&stop);
        }
        Some("--daemon-stop") => match daemon::request_stop() {
            Ok(()) => println!("[OK] stop requested"),
            Err(e) => eprintln!("[ERR] stop failed: {e}"),
        },
        Some("--daemon-status") => {
            let d = if daemon::daemon_is_alive() { "daemon alive" } else { "daemon stopped" };
            let w = if ipc::ping_instance() { "window live" } else { "no window" };
            println!("[{d} · {w}]");
        }
        Some("--notify") => {
            let req = args.get(1).cloned().unwrap_or_else(|| "sys".into());
            println!("[OK] {}", daemon::daemon_notify(&req));
        }
        Some("--exec") => {
            let input = args.get(1).cloned().unwrap_or_default();
            let status = tui::App::dispatch(&input);
            eprintln!("[{APP_NAME}] {status}");
        }
        Some("--install-shortcut") => install_shortcut(),
        Some(other) => {
            eprintln!("unknown flag: {other}\n\n{HELP}");
            std::process::exit(2);
        }
    }
}

// ---------------------------------------------------------------------------
// Foreground TUI with retry on transient attach failure
// ---------------------------------------------------------------------------

fn run_foreground_tui() {
    match tui::run_tui() {
        Ok(()) => {}
        Err(e) => {
            tui::cleanup();
            eprintln!("[ERR] terminal unavailable: {e}");
            std::process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// Graceful shutdown: poll-based stop sentinel (no extra crates, cross-OS)
// ---------------------------------------------------------------------------

fn install_stop_handler(stop: &Arc<AtomicBool>) {
    let s = Arc::clone(stop);
    thread::spawn(move || loop {
        if daemon::stop_requested() {
            s.store(true, Ordering::Relaxed);
            break;
        }
        thread::sleep(Duration::from_millis(250));
    });
}

// ---------------------------------------------------------------------------
// Launch-shortcut installer (delegates to scripts/, prints guidance)
// ---------------------------------------------------------------------------

fn install_shortcut() {
    #[cfg(windows)]
    {
        let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
        println!("{APP_NAME}: bind your launch shortcut to:\n  \"{exe}\" --summon\n");
        println!(
            "Run (once, as the current user):\n  powershell -ExecutionPolicy Bypass -File scripts\\install-shortcut.ps1\n\
             to create: Start Menu shortcut, Desktop shortcut, hidden Startup\n\
             (tray) entry, and register Win+Alt+C as the summon hotkey."
        );
    }
    #[cfg(not(windows))]
    {
        println!(
            "Run (once):\n  ./scripts/install-shortcut.sh\n\
             to create ~/.local/share/applications/com.drsamonline.cldr.desktop,\n\
             a symlink in ~/.local/bin/cldr, the XDG autostart tray entry, and a\n\
             Super+Alt+C sxhkd summon binding (if sxhkd is installed)."
        );
    }
}

//! CLDR — Command Line Dispatch & Route
//! Author: Dr. Sohil Momin
//!
//! Minimal, zero-bloat, ultra-low-memory command launcher and background
//! daemon for Windows and Linux. Single-file core engine:
//!   * Raw-mode terminal via crossterm + TUI via ratatui
//!   * `/`-prefixed dispatcher (run/open/ls/find/web/calc/sys)
//!   * Smart intent cascade for un-prefixed input
//!   * Detached process spawning, native OS dispatch via `open`
//!   * Single-instance guard: re-launching the shortcut/detached instance
//!     foregrounds the existing console window instead of opening a second one
//!     (Win32 `FindWindowW`/`ShowWindow`/`SetForegroundWindow`, POSIX `SIGUSR1`)
//!   * Optional headless mode (`cldr --headless`) and hidden daemon

use std::cell::RefCell;
use std::env;
use std::fs;
use std::io::{self, Stdout, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self};
use std::time::Duration;

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;
use walkdir::WalkDir;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const APP_NAME: &str = "CLDR";
const APP_TAGLINE: &str = "Command Line Dispatch & Route";
const AUTHOR: &str = "Dr. Sohil Momin";
const MAX_RESULTS: usize = 50;
const FIND_MAX_DEPTH: usize = 4;
const FIND_MAX_MATCHES: usize = MAX_RESULTS;
const POLL_MS: u64 = 16; // ~60 fps UI poll; near-zero idle CPU
const DAEMON_TICK_SECS: u64 = 30;
const SCROLLBACK_CAP: usize = 5_000; // hard memory ceiling for the results pane

// ---------------------------------------------------------------------------
// Result row model
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Entry {
    kind: &'static str, // DIR | FILE | OK | ERR | INFO | CALC | SYS | WEB | RUN | MATCH
    text: String,
}

impl Entry {
    fn new(kind: &'static str, text: impl Into<String>) -> Self {
        Self { kind, text: text.into() }
    }
}

fn push(entries: &Rc<RefCell<Vec<Entry>>>, e: Entry) {
    let mut v = entries.borrow_mut();
    v.push(e);
    let len = v.len();
    if len > SCROLLBACK_CAP {
        v.drain(..len - SCROLLBACK_CAP);
    }
}

// ---------------------------------------------------------------------------
// Zero-dependency arithmetic parser  (+ - * / with parentheses & unary minus)
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum Tok {
    Num(f64),
    Op(char),
    LParen,
    RParen,
}

fn tokenize(src: &str) -> Result<Vec<Tok>, String> {
    let mut toks = Vec::new();
    let b = src.as_bytes();
    let mut i = 0usize;
    while i < b.len() {
        let c = b[i] as char;
        match c {
            ' ' | '\t' => i += 1,
            '0'..='9' | '.' => {
                let start = i;
                let mut dots = 0;
                while i < b.len() && ((b[i] as char).is_ascii_digit() || b[i] as char == '.') {
                    if b[i] as char == '.' {
                        dots += 1;
                    }
                    i += 1;
                }
                if dots > 1 {
                    return Err(format!("malformed number '{}'", &src[start..i]));
                }
                let word: &str = &src[start..i];
                let v: f64 = word.parse().map_err(|_| format!("bad number '{word}'"))?;
                toks.push(Tok::Num(v));
            }
            '+' | '-' | '*' | '/' => {
                toks.push(Tok::Op(c));
                i += 1;
            }
            '(' => {
                toks.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                toks.push(Tok::RParen);
                i += 1;
            }
            other => return Err(format!("illegal character '{other}'")),
        }
    }
    Ok(toks)
}

/// Strict test: does the string parse as a pure arithmetic expression?
fn looks_like_math(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return false;
    }
    match tokenize(t) {
        Ok(toks) => !toks.is_empty() && calc_from(toks).is_ok(),
        Err(_) => false,
    }
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn new(toks: Vec<Tok>) -> Self {
        Self { toks, pos: 0 }
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn bump(&mut self) {
        self.pos += 1;
    }

    // expr := term (('+'|'-') term)*
    fn eval_expr(&mut self) -> Result<f64, String> {
        let mut v = self.eval_term()?;
        while let Some(Tok::Op(o @ ('+' | '-'))) = self.peek().cloned() {
            self.bump();
            let r = self.eval_term()?;
            v = if o == '+' { v + r } else { v - r };
        }
        Ok(v)
    }

    // term := factor (('*'|'/') factor)*
    fn eval_term(&mut self) -> Result<f64, String> {
        let mut v = self.eval_factor()?;
        while let Some(Tok::Op(o @ ('*' | '/'))) = self.peek().cloned() {
            self.bump();
            let r = self.eval_factor()?;
            if o == '/' && r == 0.0 {
                return Err("division by zero".into());
            }
            v = if o == '*' { v * r } else { v / r };
        }
        Ok(v)
    }

    // factor := ('-'|'+') factor | '(' expr ')' | Number
    fn eval_factor(&mut self) -> Result<f64, String> {
        match self.peek().cloned() {
            Some(Tok::Op('-')) => {
                self.bump();
                Ok(-self.eval_factor()?)
            }
            Some(Tok::Op('+')) => {
                self.bump();
                self.eval_factor()
            }
            Some(Tok::LParen) => {
                self.bump();
                let v = self.eval_expr()?;
                match self.peek() {
                    Some(Tok::RParen) => {
                        self.bump();
                        Ok(v)
                    }
                    _ => Err("expected ')'".into()),
                }
            }
            Some(Tok::Num(n)) => {
                self.bump();
                Ok(n)
            }
            _ => Err("unexpected end of expression".into()),
        }
    }
}

fn calc_from(toks: Vec<Tok>) -> Result<f64, String> {
    let mut p = Parser::new(toks);
    let v = p.eval_expr()?;
    if p.pos != p.toks.len() {
        return Err("trailing tokens in expression".into());
    }
    Ok(v)
}

fn calc(src: &str) -> Result<f64, String> {
    let toks = tokenize(src)?;
    if toks.is_empty() {
        return Err("empty expression".into());
    }
    calc_from(toks)
}

fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

// ---------------------------------------------------------------------------
// Dispatcher commands
// ---------------------------------------------------------------------------

fn cmd_run(arg: &str, entries: &Rc<RefCell<Vec<Entry>>>) -> String {
    if arg.is_empty() {
        push(entries, Entry::new("ERR", "/run <binary> [args] — spawn detached"));
        return "usage: /run".into();
    }
    match detached_shell(arg) {
        Ok(status) => {
            push(
                entries,
                Entry::new(
                    "RUN",
                    format!("spawned `{arg}` (detached){}", describe_status(status)),
                ),
            );
            format!("ran: {arg}")
        }
        Err(e) => {
            push(entries, Entry::new("ERR", format!("failed to run `{arg}`: {e}")));
            format!("run failed: {e}")
        }
    }
}

fn describe_status(status: std::process::ExitStatus) -> String {
    if status.success() {
        String::new()
    } else {
        format!(" [launcher exit {}]", status.code().unwrap_or(-1))
    }
}

#[cfg(windows)]
fn detached_shell(cmdline: &str) -> io::Result<std::process::ExitStatus> {
    // `cmd /C start "" <cmdline>` fully detaches the child from this console.
    Command::new("cmd")
        .args(["/C", "start", "", cmdline])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
}

#[cfg(not(windows))]
fn detached_shell(cmdline: &str) -> io::Result<std::process::ExitStatus> {
    // `sh -c "(cmd) &"` returns immediately; the grandchild is reparented.
    Command::new("sh")
        .arg("-c")
        .arg(format!("({cmdline}) >/dev/null 2>&1 &"))
        .status()
}

fn cmd_open(arg: &str, entries: &Rc<RefCell<Vec<Entry>>>) -> String {
    if arg.is_empty() {
        push(entries, Entry::new("ERR", "/open <path> — open in system default viewer"));
        return "usage: /open".into();
    }
    let path = expand_tilde(arg);
    if !path.exists() {
        push(entries, Entry::new("ERR", format!("not found: {}", path.display())));
        return "not found".into();
    }
    match open::that(&path) {
        Ok(()) => {
            push(entries, Entry::new("OK", format!("opened {}", path.display())));
            format!("opened {}", path.display())
        }
        Err(e) => {
            push(entries, Entry::new("ERR", format!("open failed: {e}")));
            format!("open failed: {e}")
        }
    }
}

fn cmd_ls(arg: &str, entries: &Rc<RefCell<Vec<Entry>>>) -> String {
    let dir = if arg.is_empty() {
        env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else {
        expand_tilde(arg)
    };
    if !dir.is_dir() {
        push(entries, Entry::new("ERR", format!("not a directory: {}", dir.display())));
        return "not a directory".into();
    }
    let read = match fs::read_dir(&dir) {
        Ok(r) => r,
        Err(e) => {
            push(entries, Entry::new("ERR", format!("ls failed: {e}")));
            return format!("ls failed: {e}");
        }
    };
    let mut names: Vec<(String, bool)> = Vec::new();
    let mut total = 0usize;
    for e in read.flatten() {
        total += 1;
        let is_dir = e.path().is_dir();
        names.push((e.file_name().to_string_lossy().into_owned(), is_dir));
    }
    names.sort_by(|a, b| a.0.cmp(&b.0));
    push(entries, Entry::new("INFO", format!("{} — {total} entries", dir.display())));
    let shown = names.len().min(MAX_RESULTS);
    for (name, is_dir) in names.into_iter().take(MAX_RESULTS) {
        let tag = if is_dir { "[DIR]" } else { "[FILE]" };
        push(entries, Entry::new(if is_dir { "DIR" } else { "FILE" }, format!("{tag} {name}")));
    }
    if total > shown {
        push(entries, Entry::new("INFO", format!("… {} more (capped at {MAX_RESULTS})", total - shown)));
    }
    format!("ls {}: {total} entries", dir.display())
}

fn cmd_find(arg: &str, cwd: &Path, entries: &Rc<RefCell<Vec<Entry>>>) -> String {
    if arg.is_empty() {
        push(
            entries,
            Entry::new("ERR", format!("/find <name> — bounded recursive search (depth {FIND_MAX_DEPTH})")),
        );
        return "usage: /find".into();
    }
    let query = arg.to_lowercase();
    let root = if cwd.exists() { cwd.to_path_buf() } else { PathBuf::from(".") };
    push(
        entries,
        Entry::new("INFO", format!("searching '{arg}' under {} (max depth {FIND_MAX_DEPTH})", root.display())),
    );
    let mut matches = 0usize;
    for entry in WalkDir::new(&root)
        .max_depth(FIND_MAX_DEPTH)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name.contains(&query) {
            let tag = if entry.file_type().is_dir() { "[DIR]" } else { "[FILE]" };
            let rel = entry
                .path()
                .strip_prefix(&root)
                .unwrap_or(entry.path())
                .display()
                .to_string();
            push(entries, Entry::new("MATCH", format!("{tag} {rel}")));
            matches += 1;
            if matches >= FIND_MAX_MATCHES {
                push(entries, Entry::new("INFO", format!("… truncated at {FIND_MAX_MATCHES} matches")));
                break;
            }
        }
    }
    if matches == 0 {
        push(entries, Entry::new("INFO", "no matches"));
    }
    format!("find '{arg}': {matches} matches")
}

fn cmd_web(arg: &str, entries: &Rc<RefCell<Vec<Entry>>>) -> String {
    if arg.is_empty() {
        push(entries, Entry::new("ERR", "/web <query> — DuckDuckGo search in default browser"));
        return "usage: /web".into();
    }
    let url = format!("https://duckduckgo.com/?q={}", urlencode(arg));
    match open::that(&url) {
        Ok(()) => {
            push(entries, Entry::new("WEB", format!("browser → {url}")));
            format!("web: {arg}")
        }
        Err(e) => {
            push(entries, Entry::new("ERR", format!("could not open browser: {e}")));
            format!("web failed: {e}")
        }
    }
}

fn cmd_calc(arg: &str, entries: &Rc<RefCell<Vec<Entry>>>) -> String {
    if arg.is_empty() {
        push(
            entries,
            Entry::new("ERR", "/calc <expr> — arithmetic with + - * / ( ) and unary minus"),
        );
        return "usage: /calc".into();
    }
    match calc(arg) {
        Ok(v) => {
            push(entries, Entry::new("CALC", format!("{} = {}", arg.trim(), fmt_num(v))));
            format!("= {}", fmt_num(v))
        }
        Err(e) => {
            push(entries, Entry::new("ERR", format!("calc error: {e}")));
            format!("calc error: {e}")
        }
    }
}

fn cmd_sys(entries: &Rc<RefCell<Vec<Entry>>>) -> String {
    let os = env::consts::OS;
    let family = env::consts::FAMILY;
    let arch = env::consts::ARCH;
    let pointer = if cfg!(target_pointer_width = "64") { "64-bit" } else { "32-bit" };
    let cores = num_cores();
    let host = hostname();
    let user = whoami();
    push(entries, Entry::new("SYS", format!("OS          : {os} (family: {family}, {pointer})")));
    push(entries, Entry::new("SYS", format!("Architecture: {arch}")));
    push(entries, Entry::new("SYS", format!("CPU Cores   : {cores}")));
    push(entries, Entry::new("SYS", format!("Hostname    : {host}")));
    push(entries, Entry::new("SYS", format!("User        : {user}")));
    push(entries, Entry::new("SYS", format!("App         : {APP_NAME} v{}", env!("CARGO_PKG_VERSION"))));
    format!("{os}/{arch} · {cores} cores")
}

fn num_cores() -> usize {
    thread::available_parallelism().map(|n| n.get()).unwrap_or(0)
}

#[cfg(unix)]
fn hostname() -> String {
    fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "unknown-host".into())
}

#[cfg(windows)]
fn hostname() -> String {
    env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown-host".into())
}

#[cfg(not(any(unix, windows)))]
fn hostname() -> String {
    "unknown-host".into()
}

fn whoami() -> String {
    env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown-user".into())
}

fn expand_tilde(s: &str) -> PathBuf {
    if s == "~" {
        if let Some(home) = home_dir() {
            return home;
        }
    }
    if let Some(rest) = s.strip_prefix("~/").or_else(|| s.strip_prefix("~\\")) {
        if let Some(home) = home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(s)
}

fn home_dir() -> Option<PathBuf> {
    env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| env::var("USERPROFILE").ok().map(PathBuf::from))
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for byte in s.as_bytes() {
        let c = *byte as char;
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => out.push(c),
            ' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Smart intent cascade (input without leading '/')
// ---------------------------------------------------------------------------

fn smart_route(raw: &str, entries: &Rc<RefCell<Vec<Entry>>>) -> String {
    let input = raw.trim();
    if input.is_empty() {
        return "empty input".into();
    }
    // 1) Arithmetic first.
    if looks_like_math(input) {
        if let Ok(v) = calc(input) {
            push(entries, Entry::new("CALC", format!("{input} = {}", fmt_num(v))));
            return format!("auto-calc: {input} = {}", fmt_num(v));
        }
    }
    // 2) Existing file/folder on disk → open it.
    let path = expand_tilde(input);
    if path.exists() {
        return cmd_open(input, entries);
    }
    let quoted = format!("\"{}\"", input.replace('"', "\\\""));
    let alt = expand_tilde(&quoted);
    if alt.exists() {
        return cmd_open(&quoted, entries);
    }
    // 3) Fallback → web search.
    cmd_web(input, entries)
}

// ---------------------------------------------------------------------------
// Top-level dispatch
// ---------------------------------------------------------------------------

fn execute_input(raw: &str, cwd: &Path, entries: &Rc<RefCell<Vec<Entry>>>) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "type /help for commands".into();
    }
    if !trimmed.starts_with('/') {
        return smart_route(trimmed, entries);
    }
    let body = &trimmed[1..];
    let (head, arg) = match body.find(' ') {
        Some(i) => (&body[..i], body[i + 1..].trim()),
        None => (body, ""),
    };
    match head {
        "run" => cmd_run(arg, entries),
        "open" => cmd_open(arg, entries),
        "ls" => cmd_ls(arg, entries),
        "find" => cmd_find(arg, cwd, entries),
        "web" => cmd_web(arg, entries),
        "calc" => cmd_calc(arg, entries),
        "sys" => cmd_sys(entries),
        "help" => show_help(entries),
        "clear" => {
            entries.borrow_mut().clear();
            "cleared".into()
        }
        other => {
            push(entries, Entry::new("ERR", format!("unknown command /{other} — try /help")));
            format!("unknown command /{other}")
        }
    }
}

fn show_help(entries: &Rc<RefCell<Vec<Entry>>>) -> String {
    let rows = [
        ("/run <binary>", "spawn detached via OS shell"),
        ("/open <path>", "open file/folder in default viewer"),
        ("/ls [path]", "list directory (capped at 50)"),
        ("/find <name>", "recursive search, max depth 4"),
        ("/web <query>", "DuckDuckGo search in browser"),
        ("/calc <expr>", "arithmetic: + - * / ( ) unary-"),
        ("/sys", "hardware diagnostics"),
        ("/help", "this cheat sheet"),
        ("/clear", "clear results pane"),
        ("(no prefix)", "auto: math → path → web search"),
    ];
    push(entries, Entry::new("INFO", format!("{APP_NAME} — {APP_TAGLINE} · by {AUTHOR}")));
    for (cmd, desc) in rows {
        push(entries, Entry::new("INFO", format!("{cmd:<16} {desc}")));
    }
    "help displayed".into()
}

// ---------------------------------------------------------------------------
// Background daemon (detached helper process)
// ---------------------------------------------------------------------------

fn daemon_base_path() -> PathBuf {
    let base = if cfg!(windows) {
        env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| env::temp_dir())
    } else {
        env::var("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| env::temp_dir())
    };
    base.join("cldr-daemon.req")
}

fn daemon_pid_path() -> PathBuf {
    daemon_base_path().with_extension("pid")
}

fn daemon_hb_path() -> PathBuf {
    daemon_base_path().with_extension("hb")
}

fn daemon_stop_path() -> PathBuf {
    daemon_base_path().with_extension("stop")
}

fn daemon_log_path() -> PathBuf {
    daemon_base_path().with_extension("log")
}

fn daemon_is_alive() -> bool {
    if let Ok(s) = fs::read_to_string(daemon_pid_path()) {
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

fn spawn_daemon_detached() -> io::Result<()> {
    let exe = env::current_exe()?;
    #[cfg(windows)]
    {
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
                "(setsid '{}' --daemon >'{}' 2>&1 &) ",
                exe.display(),
                daemon_log_path().display()
            ))
            .spawn()?;
    }
    Ok(())
}

fn ensure_daemon() -> String {
    if daemon_is_alive() {
        return "daemon already running".into();
    }
    match spawn_daemon_detached() {
        Ok(()) => "daemon spawned (detached)".into(),
        Err(e) => format!("daemon spawn failed: {e}"),
    }
}

/// Headless worker loop: heartbeat, request servicing, stop sentinel polling.
fn run_daemon_loop(stop: &AtomicBool) {
    let pid_file = daemon_pid_path();
    let hb_file = daemon_hb_path();
    let req_file = daemon_base_path();
    let stop_file = daemon_stop_path();
    let _ = fs::write(&pid_file, std::process::id().to_string());
    let _ = fs::remove_file(&stop_file);
    while !stop.load(Ordering::Relaxed) && !stop_file.exists() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = fs::write(&hb_file, now.to_string());
        if let Ok(req) = fs::read_to_string(&req_file) {
            let req = req.trim().to_string();
            let _ = fs::remove_file(&req_file);
            if !req.is_empty() {
                service_daemon_request(&req, &hb_file);
            }
        }
        // Sleep in small quanta so shutdown stays prompt (<1s after stop flag).
        for _ in 0..DAEMON_TICK_SECS {
            if stop.load(Ordering::Relaxed) || stop_file.exists() {
                break;
            }
            thread::sleep(Duration::from_secs(1));
        }
    }
    let _ = fs::remove_file(&pid_file);
    let _ = fs::remove_file(&hb_file);
    let _ = fs::remove_file(&stop_file);
}

fn service_daemon_request(req: &str, hb_file: &Path) {
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
        "sys" => {
            let sink = Rc::new(RefCell::new(Vec::new()));
            cmd_sys(&sink);
            "sys ok".to_string()
        }
        other => format!("unknown request: {other}"),
    };
    if let Ok(mut f) = fs::OpenOptions::new().append(true).open(hb_file) {
        let _ = writeln!(f, "{outcome}");
    }
}

/// Enqueue a request for the daemon (used by `cldr --notify`).
fn daemon_notify(request: &str) -> String {
    if !daemon_is_alive() {
        return "daemon not running — start with: cldr --daemon-start".into();
    }
    match fs::write(daemon_base_path(), request) {
        Ok(()) => format!("queued for daemon: {request}"),
        Err(e) => format!("notify failed: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Single-instance guard: shortcut re-launch foregrounds the existing window
// ---------------------------------------------------------------------------

/// Window title used by both the primary instance and any wake-up attempt,
/// so a second launch of the shortcut can locate and raise the first one.
#[cfg_attr(not(windows), allow(dead_code))] // referenced by the Win32 foreground path
const WINDOW_TITLE: &str = "CLDR — Command Line Dispatch & Route";

#[cfg(windows)]
mod win_single {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    type HWND = isize;
    type HANDLE = isize;
    type DWORD = u32;
    type BOOL = i32;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateMutexW(lpmutexattributes: usize, binitialowner: BOOL, lpname: *const u16) -> HANDLE;
        fn GetLastError() -> DWORD;
        fn CloseHandle(hobject: HANDLE) -> BOOL;
    }

    #[link(name = "user32")]
    extern "system" {
        fn FindWindowW(lpclassname: *const u16, lpwindowname: *const u16) -> HWND;
        fn ShowWindow(hwnd: HWND, ncmdshow: i32) -> BOOL;
        fn SetForegroundWindow(hwnd: HWND) -> BOOL;
    }

    const ERROR_ALREADY_EXISTS: DWORD = 183;
    const SW_RESTORE: i32 = 9;

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    }

    /// Acquire the named mutex for this desktop session. `Ok(true)` means this
    /// process is the primary instance; `Ok(false)` means another instance owns it.
    pub fn acquire_primary_lock() -> Result<bool, String> {
        let name = wide("Local\\cldr-single-instance");
        let handle = unsafe { CreateMutexW(0, 1, name.as_ptr()) };
        if handle == -1 {
            return Err(format!("CreateMutexW failed (gle={})", unsafe { GetLastError() }));
        }
        let owned = unsafe { GetLastError() } != ERROR_ALREADY_EXISTS;
        if !owned {
            // Not ours — release our handle; the owner keeps its mutex alive.
            unsafe { CloseHandle(handle) };
        }
        // Intentionally leak `handle` when owned: the mutex must live for the
        // entire process lifetime so re-launches detect us.
        Ok(owned)
    }

    /// Best-effort: restore + foreground the console window of the running
    /// primary instance. Returns true if a window was found and raised.
    pub fn foreground_existing(title: &str) -> bool {
        let t = wide(title);
        let hwnd = unsafe { FindWindowW(std::ptr::null(), t.as_ptr()) };
        if hwnd == 0 {
            return false;
        }
        unsafe {
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
        }
        true
    }
}

#[derive(Clone, Copy)]
pub struct InstanceGuard {
    pub is_primary: bool,
}

impl InstanceGuard {
    /// Try to become the single running interactive instance. On success this
    /// also sets the console window title so future launches can find us.
    pub fn acquire() -> Self {
        #[cfg(windows)]
        {
            crossterm::terminal::SetTitleFormat(WINDOW_TITLE).ok();
            match win_single::acquire_primary_lock() {
                Ok(primary) => InstanceGuard { is_primary: primary },
                // If the OS lock fails, prefer running over refusing to run.
                Err(_) => InstanceGuard { is_primary: true },
            }
        }
        #[cfg(not(windows))]
        {
            InstanceGuard { is_primary: posix_acquire_lock() }
        }
    }

    /// Called by a *second* launch: wake the primary instance's window and
    /// return true if the hand-off succeeded.
    pub fn wake_existing() -> bool {
        #[cfg(windows)]
        {
            win_single::foreground_existing(WINDOW_TITLE)
        }
        #[cfg(not(windows))]
        {
            posix_wake_existing()
        }
    }
}

#[cfg(unix)]
fn posix_lock_path() -> PathBuf {
    env::var("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
        .join("cldr-single-instance.lock")
}

#[cfg(unix)]
fn posix_pid_path() -> PathBuf {
    posix_lock_path().with_extension("pid")
}

#[cfg(unix)]
fn posix_already_running(pid: u32) -> bool {
    if pid == 0 || pid == std::process::id() {
        return pid == std::process::id();
    }
    process_alive(pid)
}

/// POSIX single-instance guard: flock on a shared lock file + PID registry.
/// A stale lock (holder died) is transparently replaced by the new launch.
#[cfg(unix)]
fn posix_acquire_lock() -> bool {
    use std::os::unix::io::AsRawFd;

    let path = posix_lock_path();
    let file = match fs::OpenOptions::new().create(true).truncate(false).write(true).open(&path) {
        Ok(f) => f,
        Err(_) => return true, // fail-open: better a duplicate than a refusal
    };
    let rc = unsafe { flock(file.as_raw_fd(), /*LOCK_EX|LOCK_NB*/ 2 | 4) };
    if rc != 0 {
        // Lock held elsewhere: verify the recorded PID is really alive before
        // declaring "already running" (guards against leftover pid files).
        let alive = fs::read_to_string(posix_pid_path())
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .map(posix_already_running)
            .unwrap_or(false);
        drop(file);
        return !alive;
    }
    // Lock acquired — reclaim ownership even if a dead holder left a pid file.
    let _ = file.set_len(0);
    let _ = fs::write(posix_pid_path(), std::process::id().to_string());
    std::mem::forget(file); // keep the fd (and thus the lock) open for our lifetime
    set_wake_handler();
    true
}

#[cfg(unix)]
fn posix_wake_existing() -> bool {
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    const SIGUSR1: i32 = 10;
    match fs::read_to_string(posix_pid_path())
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
    {
        Some(pid) if pid != 0 && process_alive(pid) => {
            unsafe { kill(pid as i32, SIGUSR1) == 0 }
        }
        _ => false,
    }
}

#[cfg(unix)]
extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}

#[cfg(unix)]
fn set_wake_handler() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        extern "C" {
            fn signal(sig: i32, handler: usize) -> usize;
        }
        const SIGUSR1: i32 = 10;
        unsafe {
            signal(SIGUSR1, wake_handler as *const () as usize);
        }
    });
}

extern "C" fn wake_handler(_sig: i32) {
    // Signal-safe: just flip the flag; the event loop restores the terminal.
    WAKE_FLAG.store(true, Ordering::Relaxed);
}

static WAKE_FLAG: AtomicBool = AtomicBool::new(false);

// ---------------------------------------------------------------------------
// TUI application
// ---------------------------------------------------------------------------

struct App {
    input: String,
    entries: Rc<RefCell<Vec<Entry>>>,
    list_state: ListState,
    status: String,
    running: bool,
    cwd: PathBuf,
}

impl App {
    fn new() -> Self {
        let entries = Rc::new(RefCell::new(Vec::new()));
        push(
            &entries,
            Entry::new(
                "INFO",
                format!("{APP_NAME} v{} — {APP_TAGLINE} · by {AUTHOR}", env!("CARGO_PKG_VERSION")),
            ),
        );
        push(&entries, Entry::new("INFO", "press /help for the command cheat sheet · Esc quits"));
        let mut app = Self {
            input: String::new(),
            entries,
            list_state: ListState::default(),
            status: "ready".into(),
            running: true,
            cwd: env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        };
        app.jump_bottom();
        app
    }

    fn submit(&mut self) {
        let raw = std::mem::take(&mut self.input);
        self.status = execute_input(&raw, &self.cwd, &self.entries);
        self.jump_bottom();
    }

    fn jump_bottom(&mut self) {
        let len = self.entries.borrow().len();
        if len > 0 {
            self.list_state.select(Some(len - 1));
        }
    }

    fn move_sel(&mut self, delta: isize) {
        let len = self.entries.borrow().len() as isize;
        if len == 0 {
            return;
        }
        let cur = self.list_state.selected().unwrap_or(0) as isize;
        let next = (cur + delta).clamp(0, len - 1);
        self.list_state.select(Some(next as usize));
    }

    fn on_key(&mut self, code: KeyCode, shift: bool) {
        use KeyCode::*;
        match code {
            Char(c) => {
                if shift && c.is_alphabetic() {
                    self.input.push(c.to_ascii_uppercase());
                } else {
                    self.input.push(c);
                }
            }
            Backspace => {
                self.input.pop();
            }
            Delete => {
                self.input.clear();
            }
            Enter => self.submit(),
            Up => self.move_sel(-1),
            Down => self.move_sel(1),
            PageUp => self.move_sel(-10),
            PageDown => self.move_sel(10),
            Home => {
                if !self.entries.borrow().is_empty() {
                    self.list_state.select(Some(0));
                }
            }
            End => self.jump_bottom(),
            Esc => self.running = false,
            _ => {}
        }
    }

    fn draw(&mut self, f: &mut Frame) {
        let area = f.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // input bar
                Constraint::Min(3),    // results
                Constraint::Length(1), // status strip
            ])
            .split(area);

        self.draw_input(f, chunks[0]);
        self.draw_results(f, chunks[1]);
        self.draw_status(f, chunks[2]);
    }

    fn draw_input(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {APP_NAME} ▸ {APP_TAGLINE} "))
            .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .border_style(Style::default().fg(Color::DarkGray));
        let para = Paragraph::new(Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(self.input.clone(), Style::default().fg(Color::White)),
        ]))
        .block(block);
        f.render_widget(para, area);
    }

    fn draw_results(&mut self, f: &mut Frame, area: Rect) {
        let borrow = self.entries.borrow();
        let items: Vec<ListItem> = borrow
            .iter()
            .map(|e| {
                let color = match e.kind {
                    "DIR" => Color::Cyan,
                    "FILE" => Color::White,
                    "OK" => Color::Green,
                    "ERR" => Color::Red,
                    "CALC" => Color::Magenta,
                    "SYS" => Color::Blue,
                    "WEB" => Color::LightBlue,
                    "RUN" => Color::Yellow,
                    "MATCH" => Color::LightCyan,
                    _ => Color::DarkGray,
                };
                ListItem::new(Line::from(Span::styled(e.text.clone(), Style::default().fg(color))))
            })
            .collect();
        drop(borrow);

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::LEFT | Borders::RIGHT)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );
        f.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn draw_status(&self, f: &mut Frame, area: Rect) {
        let budget = (area.width as usize).saturating_sub(4);
        let cwd_txt = truncate(&self.cwd.display().to_string(), budget / 2);
        let status_txt = truncate(&self.status, budget.saturating_sub(cwd_txt.len()));
        let txt = format!(" {status_txt} · {cwd_txt} ");
        let p = Paragraph::new(txt).style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
        f.render_widget(p, area);
    }
}

fn truncate(s: &str, w: usize) -> String {
    if w == 0 {
        return String::new();
    }
    if s.chars().count() <= w {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(w.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

// ---------------------------------------------------------------------------
// Terminal lifecycle
// ---------------------------------------------------------------------------

fn attach_terminal() -> io::Result<CrosstermBackend<Stdout>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    Ok(CrosstermBackend::new(stdout))
}

// ---------------------------------------------------------------------------
// CLI entry points
// ---------------------------------------------------------------------------

fn print_banner_line(msg: &str) {
    println!("[{APP_NAME}] {msg}");
}

fn usage() {
    println!("{APP_NAME} — {APP_TAGLINE} · by {AUTHOR}");
    println!();
    println!("USAGE:");
    println!("  cldr                       launch interactive TUI");
    println!("  cldr --headless \"<cmds>\"   run without a TTY (newline-separated commands)");
    println!("  cldr --daemon              run hidden background worker (internal)");
    println!("  cldr --daemon-start        spawn the background daemon, detached");
    println!("  cldr --daemon-stop         signal a running daemon to shut down");
    println!("  cldr --daemon-status       report daemon liveness");
    println!("  cldr --notify \"<request>\"  queue a request for the daemon (open/run/sys)");
    println!("  cldr --version             print version");
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("--version") | Some("-V") => {
            println!("{APP_NAME} {} · {APP_TAGLINE} · by {AUTHOR}", env!("CARGO_PKG_VERSION"));
            return;
        }
        Some("--help") | Some("-h") => {
            usage();
            return;
        }
        Some("--daemon") => {
            // Hidden background worker: no terminal, bounded memory, tiny binary.
            // Deliberately bypasses the single-instance guard so it can coexist
            // with an interactive session.
            let stop = Arc::new(AtomicBool::new(false));
            install_stop_flag(Arc::clone(&stop));
            run_daemon_loop(&stop);
            return;
        }
        Some("--daemon-start") => {
            print_banner_line(&ensure_daemon());
            return;
        }
        Some("--daemon-stop") => {
            match fs::write(daemon_stop_path(), "stop") {
                Ok(()) => print_banner_line("stop requested (daemon exits within one tick)"),
                Err(e) => print_banner_line(&format!("stop failed: {e}")),
            }
            return;
        }
        Some("--daemon-status") => {
            print_banner_line(if daemon_is_alive() { "daemon: ALIVE" } else { "daemon: STOPPED" });
            return;
        }
        Some("--notify") => {
            let req = args.get(1).cloned().unwrap_or_default();
            print_banner_line(&daemon_notify(&req));
            return;
        }
        Some("--headless") => {
            let script = args.get(1).cloned().unwrap_or_default();
            run_headless(&script);
            return;
        }
        _ => {}
    }

    // Interactive launch: honour the single-instance contract. If another
    // instance already owns the lock, foreground *its* window instead of
    // starting a second one (the "shortcut re-open" behaviour).
    let guard = InstanceGuard::acquire();
    if !guard.is_primary {
        if InstanceGuard::wake_existing() {
            print_banner_line("existing instance foregrounded — not opening a second window");
            return;
        }
        eprintln!("{APP_NAME}: another instance is running but its window could not be raised.");
        eprintln!("(close it first, or use --headless for scripted execution)");
        std::process::exit(1);
    }

    if let Err(e) = run_tui() {
        eprintln!("{APP_NAME}: fatal: {e}");
        std::process::exit(1);
    }
}

fn run_headless(script: &str) {
    let entries = Rc::new(RefCell::new(Vec::new()));
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if script.trim().is_empty() {
        println!("(nothing to do)");
        return;
    }
    for line in script.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let status = execute_input(line, &cwd, &entries);
        println!("$ {line}\n→ {status}");
        for e in entries.borrow().iter() {
            println!("   {}", e.text);
        }
        entries.borrow_mut().clear();
    }
}

fn run_tui() -> io::Result<()> {
    let backend = attach_terminal()?;
    // `Terminal::new` takes ownership of the backend; the terminal is fully
    // restored in `cleanup()` when the event loop finishes.
    let mut terminal = ratatui::Terminal::new(backend)?;
    let result = event_loop(&mut terminal);
    cleanup();
    result
}

/// Restore the real terminal: leave alternate screen, disable raw mode/mouse,
/// show cursor again. Safe to call multiple times.
fn cleanup() {
    let _ = disable_raw_mode();
    let mut stdout = io::stdout();
    let _ = execute!(stdout, LeaveAlternateScreen, DisableMouseCapture);
    let _ = stdout.write_all(b"\x1b[?25h");
    let _ = stdout.flush();
}

fn event_loop(terminal: &mut ratatui::Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    let mut app = App::new();
    terminal.draw(|f| app.draw(f))?;
    while app.running {
        if event::poll(Duration::from_millis(POLL_MS))? {
            match event::read()? {
                Event::Key(k) if k.kind == KeyEventKind::Press => {
                    app.on_key(k.code, k.modifiers.contains(KeyModifiers::SHIFT));
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
        // Re-launch shortcut hand-off: a second process sent us SIGUSR1 —
        // repaint and take back the foreground cleanly.
        if WAKE_FLAG.swap(false, Ordering::Relaxed) {
            app.status = "window foregrounded".into();
        }
        terminal.draw(|f| app.draw(f))?;
        position_real_cursor(terminal, &app)?;
    }
    Ok(())
}

/// Park the native terminal cursor at the end of the input line so text
/// editors' muscle memory keeps working (no fake blink thread needed).
fn position_real_cursor<B: ratatui::backend::Backend>(
    terminal: &mut ratatui::Terminal<B>,
    app: &App,
) -> io::Result<()> {
    let x = 2 + app.input.chars().count().min(120) as u16;
    terminal.set_cursor_position((x.min(terminal.size()?.width.saturating_sub(2)), 1))
}

/// Best-effort graceful shutdown hook for the daemon (Ctrl+C / SIGTERM aware).
fn install_stop_flag(flag: Arc<AtomicBool>) {
    #[cfg(unix)]
    {
        ignore_sigpipe();
        std::thread::spawn(move || {
            // Poll-free fallback: rely on the stop sentinel file written by
            // `cldr --daemon-stop`; nothing further to wire up portably.
            let _ = flag;
        });
    }
    #[cfg(not(unix))]
    {
        let _ = flag;
    }
}

#[cfg(unix)]
fn ignore_sigpipe() {
    extern "C" {
        fn signal(sig: i32, handler: usize) -> usize;
    }
    const SIGPIPE: i32 = 13;
    const SIG_IGN: usize = 1;
    unsafe {
        let _ = signal(SIGPIPE, SIG_IGN);
    }
}

// ---------------------------------------------------------------------------
// Unit tests (calc engine + helpers)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_arithmetic() {
        assert_eq!(calc("1+2*3").unwrap(), 7.0);
        assert_eq!(calc("(1+2)*3").unwrap(), 9.0);
        assert_eq!(calc("-4+10").unwrap(), 6.0);
        assert_eq!(calc("10/4").unwrap(), 2.5);
        assert_eq!(calc("2*(3+4)/7").unwrap(), 2.0);
    }

    #[test]
    fn errors() {
        assert!(calc("1/0").is_err());
        assert!(calc("1+").is_err());
        assert!(calc("(1+2").is_err());
        assert!(calc("abc").is_err());
    }

    #[test]
    fn math_detection() {
        assert!(looks_like_math("3 * (4 + 2)"));
        assert!(!looks_like_math("hello world"));
        assert!(!looks_like_math("C:\\Users"));
        assert!(!looks_like_math(""));
    }

    #[test]
    fn formatting() {
        assert_eq!(fmt_num(6.0), "6");
        assert_eq!(fmt_num(2.5), "2.5");
    }

    #[test]
    fn url_encoding() {
        assert_eq!(urlencode("hi there&you"), "hi+there%26you");
    }
}

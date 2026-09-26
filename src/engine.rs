//! ============================================================================
//! CLDR — Command Line Dispatch & Route
//! © 2026 Dr. Sohil Momin (drsamonline). All rights reserved.
//! MIT-licensed: THE ABOVE COPYRIGHT / ATTRIBUTION NOTICE MUST BE RETAINED IN
//! ALL COPIES OR SUBSTANTIAL PORTIONS OF THIS SOFTWARE. Removing this
//! watermark is a violation of the license terms.
//! ============================================================================
//!
//! Core engine: zero-dependency arithmetic parser, `/`-prefixed command
//! dispatcher (run/open/ls/find/web/calc/sys/help), and the smart intent
//! cascade (math → path → PATH binary → web fallback). Shared by the TUI,
//! the headless runner, and the Windows tray summon window.

use std::cell::RefCell;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::thread;

use walkdir::WalkDir;

pub const APP_NAME: &str = "CLDR";
pub const APP_TAGLINE: &str = "Command Line Dispatch & Route";
pub const AUTHOR: &str = "Dr. Sohil Momin";
pub const MAX_RESULTS: usize = 50;
pub const FIND_MAX_DEPTH: usize = 4;
pub const FIND_MAX_MATCHES: usize = MAX_RESULTS;
pub const SCROLLBACK_CAP: usize = 5_000; // hard memory ceiling for results panes

// ---------------------------------------------------------------------------
// Result row model
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Entry {
    pub kind: &'static str, // DIR | FILE | OK | ERR | INFO | CALC | SYS | WEB | RUN | MATCH
    pub text: String,
}

impl Entry {
    pub fn new(kind: &'static str, text: impl Into<String>) -> Self {
        Self { kind, text: text.into() }
    }
}

pub fn push(entries: &Rc<RefCell<Vec<Entry>>>, e: Entry) {
    let mut v = entries.borrow_mut();
    v.push(e);
    let len = v.len();
    if len > SCROLLBACK_CAP {
        v.drain(..len - SCROLLBACK_CAP);
    }
}

// ---------------------------------------------------------------------------
// Zero-dependency arithmetic parser (+ - * / with parentheses & unary minus)
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
pub fn looks_like_math(s: &str) -> bool {
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

pub fn calc(src: &str) -> Result<f64, String> {
    let toks = tokenize(src)?;
    if toks.is_empty() {
        return Err("empty expression".into());
    }
    calc_from(toks)
}

pub fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

// ---------------------------------------------------------------------------
// Dispatcher commands
// ---------------------------------------------------------------------------

fn describe_status(status: std::process::ExitStatus) -> String {
    if status.success() {
        String::new()
    } else {
        format!(" [launcher exit {}]", status.code().unwrap_or(-1))
    }
}

#[cfg(windows)]
pub fn detached_shell(cmdline: &str) -> io::Result<std::process::ExitStatus> {
    // `cmd /C start "" <cmdline>` fully detaches the child from this console.
    Command::new("cmd")
        .args(["/C", "start", "", cmdline])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
}

#[cfg(not(windows))]
pub fn detached_shell(cmdline: &str) -> io::Result<std::process::ExitStatus> {
    // `sh -c "(cmd) &"` returns immediately; the grandchild is reparented.
    Command::new("sh")
        .arg("-c")
        .arg(format!("({cmdline}) >/dev/null 2>&1 &"))
        .status()
}

pub fn cmd_run(arg: &str, entries: &Rc<RefCell<Vec<Entry>>>) -> String {
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

pub fn cmd_open(arg: &str, entries: &Rc<RefCell<Vec<Entry>>>) -> String {
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

pub fn cmd_ls(arg: &str, entries: &Rc<RefCell<Vec<Entry>>>) -> String {
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

pub fn cmd_find(arg: &str, cwd: &Path, entries: &Rc<RefCell<Vec<Entry>>>) -> String {
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

pub fn cmd_web(arg: &str, entries: &Rc<RefCell<Vec<Entry>>>) -> String {
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

pub fn cmd_calc(arg: &str, entries: &Rc<RefCell<Vec<Entry>>>) -> String {
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

pub fn cmd_sys(entries: &Rc<RefCell<Vec<Entry>>>) -> String {
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
    push(entries, Entry::new("SYS", format!("Author      : {AUTHOR}")));
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

pub fn expand_tilde(s: &str) -> PathBuf {
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

pub fn home_dir() -> Option<PathBuf> {
    env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| env::var("USERPROFILE").ok().map(PathBuf::from))
}

pub fn urlencode(s: &str) -> String {
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

/// Is `name` resolvable as an executable on `PATH`? (cascade step 3)
pub fn which(name: &str) -> bool {
    #[cfg(windows)]
    {
        Command::new("where")
            .arg(name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        Command::new("sh")
            .arg("-c")
            .arg(format!("command -v -- {}", shell_escape(name)))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

#[cfg(not(windows))]
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

// ---------------------------------------------------------------------------
// Smart intent cascade (input without leading '/')
// ---------------------------------------------------------------------------

pub fn smart_route(raw: &str, entries: &Rc<RefCell<Vec<Entry>>>) -> String {
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
    // 3) Named executable on PATH → launch it detached.
    if !input.contains(' ') && which(input) {
        return cmd_run(input, entries);
    }
    // 4) Fallback → web search.
    cmd_web(input, entries)
}

// ---------------------------------------------------------------------------
// Top-level dispatch
// ---------------------------------------------------------------------------

pub fn execute_input(raw: &str, cwd: &Path, entries: &Rc<RefCell<Vec<Entry>>>) -> String {
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

pub fn show_help(entries: &Rc<RefCell<Vec<Entry>>>) -> String {
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
        ("(no prefix)", "auto: math → path → PATH app → web"),
    ];
    push(entries, Entry::new("INFO", format!("{APP_NAME} — {APP_TAGLINE} · by {AUTHOR}")));
    for (cmd, desc) in rows {
        push(entries, Entry::new("INFO", format!("{cmd:<16} {desc}")));
    }
    "help displayed".into()
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

// =============================================================================
// CLDR - Command Line Dispatch & Route
// Author: Dr. Sohil Momin
// Minimal, zero-bloat command launcher and background daemon (Windows/Linux).
// Single-file core engine: raw terminal UI (crossterm + ratatui), OS dispatch
// via `open`, bounded recursive search via `walkdir`, zero-dependency calc.
// =============================================================================

use std::env;
use std::fs;
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use walkdir::WalkDir;

// -----------------------------------------------------------------------------
// Constants
// -----------------------------------------------------------------------------
const APP_NAME: &str = "CLDR";
const APP_TAGLINE: &str = "Command Line Dispatch & Route";
const MAX_RESULTS: usize = 50; // hard cap for /ls and /find output lists
const FIND_MAX_DEPTH: usize = 4; // bounded recursive walk depth for /find

// -----------------------------------------------------------------------------
// Application state
// -----------------------------------------------------------------------------
struct App {
    input: String,        // active input buffer (typed after ">")
    results: Vec<String>, // navigable result list
    selected: usize,      // cursor position within results
    status: String,       // bottom status strip message
    should_quit: bool,
}

impl App {
    fn new() -> Self {
        Self {
            input: String::new(),
            results: vec![
                format!("{} v{} - {}", APP_NAME, env!("CARGO_PKG_VERSION"), APP_TAGLINE),
                "Type / for commands. Esc quits.".to_string(),
                "Commands: /run /open /ls /find /web /calc /sys /help".to_string(),
            ],
            selected: 0,
            status: String::from("Ready."),
            should_quit: false,
        }
    }

    fn set_status<S: Into<String>>(&mut self, msg: S) {
        self.status = msg.into();
    }

    fn set_results(&mut self, items: Vec<String>) {
        self.results = items;
        self.selected = 0;
    }

    fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    fn move_down(&mut self) {
        if !self.results.is_empty() && self.selected + 1 < self.results.len() {
            self.selected += 1;
        }
    }

    /// Execute whatever is currently in the input buffer, then clear it.
    fn submit(&mut self) {
        let raw = self.input.trim().to_string();
        self.input.clear();
        if raw.is_empty() {
            return;
        }
        self.dispatch(&raw);
    }

    /// Router: leading '/' => explicit command, otherwise smart intent cascade.
    fn dispatch(&mut self, raw: &str) {
        if let Some(cmd_line) = raw.strip_prefix('/') {
            self.run_command(cmd_line);
        } else {
            self.smart_cascade(raw);
        }
    }

    fn run_command(&mut self, cmd_line: &str) {
        let mut parts = cmd_line.splitn(2, ' ');
        let verb = parts.next().unwrap_or("").trim();
        let arg = parts.next().unwrap_or("").trim();

        match verb {
            "help" | "h" | "?" => self.cmd_help(),
            "run" => self.cmd_run(arg),
            "open" => self.cmd_open(arg),
            "ls" => self.cmd_ls(arg),
            "find" => self.cmd_find(arg),
            "web" => self.cmd_web(arg),
            "calc" => self.cmd_calc(arg),
            "sys" => self.cmd_sys(),
            "clear" => {
                self.set_results(Vec::new());
                self.set_status("Cleared.");
            }
            "" => self.set_status("Empty command. Try /help"),
            other => self.set_status(format!("Unknown command: /{}. Try /help", other)),
        }
    }

    // ---- Command implementations -------------------------------------------

    fn cmd_help(&mut self) {
        self.set_results(vec![
            "/run <binary>      Launch detached process via OS shell".into(),
            "/open <path>       Open file/folder in system default viewer".into(),
            "/ls [path]         Directory listing (max 50 entries)".into(),
            "/find <name>       Recursive search, max-depth 4, substring match".into(),
            "/web <query>       Search DuckDuckGo in default browser".into(),
            "/calc <expr>       Arithmetic: + - * / ( ) e.g. (2+3)*4/2".into(),
            "/sys               Hardware diagnostics (arch, OS, cores)".into(),
            "/clear             Clear the results view".into(),
            "<no slash>         Auto-route: math -> path -> web search".into(),
            "Up/Down            Navigate results   Enter: execute   Esc: quit".into(),
        ]);
        self.set_status("Cheatsheet loaded.");
    }

    /// `/run <binary>`: spawn fully detached via the native shell handler.
    fn cmd_run(&mut self, arg: &str) {
        if arg.is_empty() {
            self.set_status("Usage: /run <binary> [args]");
            return;
        }
        match spawn_detached(arg) {
            Ok(()) => self.set_status(format!("Launched (detached): {}", arg)),
            Err(e) => self.set_status(format!("/run failed: {}", e)),
        }
    }

    /// `/open <path>`: hand off to the OS default handler.
    fn cmd_open(&mut self, arg: &str) {
        if arg.is_empty() {
            self.set_status("Usage: /open <path>");
            return;
        }
        let path = expand_tilde(arg);
        if !path.exists() {
            self.set_status(format!("Path not found: {}", arg));
            return;
        }
        match open::that(&path) {
            Ok(()) => self.set_status(format!("Opened: {}", path.display())),
            Err(e) => self.set_status(format!("/open failed: {}", e)),
        }
    }

    /// `/ls [path]`: instant capped directory listing.
    fn cmd_ls(&mut self, arg: &str) {
        let dir_str = if arg.is_empty() { ".".to_string() } else { arg.to_string() };
        let dir = expand_tilde(&dir_str);
        match fs::read_dir(&dir) {
            Ok(entries) => {
                let mut items: Vec<String> = Vec::new();
                let mut truncated = false;
                for entry in entries.flatten() {
                    if items.len() >= MAX_RESULTS {
                        truncated = true;
                        break;
                    }
                    let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
                    let tag = if is_dir { "[DIR] " } else { "[FILE]" };
                    let name = entry.file_name().to_string_lossy().into_owned();
                    items.push(format!("{} {}", tag, name));
                }
                items.sort();
                if truncated {
                    items.push(format!("... (capped at {} entries)", MAX_RESULTS));
                }
                if items.is_empty() {
                    items.push("(empty directory)".into());
                }
                self.set_results(items);
                self.set_status(format!("LS {}", dir.display()));
            }
            Err(e) => self.set_status(format!("/ls failed: {}", e)),
        }
    }

    /// `/find <name>`: bounded recursive walk matching a substring, case-insensitive.
    fn cmd_find(&mut self, arg: &str) {
        if arg.is_empty() {
            self.set_status(format!("Usage: /find <name>  (searches cwd, depth <= {})", FIND_MAX_DEPTH));
            return;
        }
        let query = arg.to_lowercase();
        let root = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut hits: Vec<String> = Vec::new();
        for entry in WalkDir::new(&root)
            .max_depth(FIND_MAX_DEPTH)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                // skip hidden dirs for speed & low memory churn
                let name = e.file_name().to_string_lossy();
                !(e.depth() > 0 && name.starts_with('.'))
            })
            .flatten()
        {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.contains(&query) {
                let tag = if entry.file_type().is_dir() { "[DIR] " } else { "[FILE]" };
                hits.push(format!("{} {}", tag, entry.path().display()));
                if hits.len() >= MAX_RESULTS {
                    hits.push(format!("... (capped at {} matches)", MAX_RESULTS));
                    break;
                }
            }
        }
        if hits.is_empty() {
            hits.push(format!("No matches for \"{}\" (depth <= {})", arg, FIND_MAX_DEPTH));
        }
        self.set_results(hits);
        self.set_status(format!("FIND \"{}\" complete.", arg));
    }

    /// `/web <query>`: DuckDuckGo search in the default browser.
    fn cmd_web(&mut self, arg: &str) {
        if arg.is_empty() {
            self.set_status("Usage: /web <query>");
            return;
        }
        let url = format!("https://duckduckgo.com/?q={}", url_encode(arg));
        match open::that(&url) {
            Ok(()) => self.set_status(format!("Web search: {}", arg)),
            Err(e) => self.set_status(format!("/web failed: {}", e)),
        }
    }

    /// `/calc <expr>`: deterministic zero-dependency arithmetic evaluator.
    fn cmd_calc(&mut self, arg: &str) {
        if arg.is_empty() {
            self.set_status("Usage: /calc <expr>   e.g. /calc (2+3)*4/2");
            return;
        }
        match eval_expr(arg) {
            Ok(value) => {
                let pretty = format_number(value);
                self.set_results(vec![format!("{} = {}", arg, pretty)]);
                self.set_status("Calc OK.");
            }
            Err(msg) => self.set_status(format!("Calc error: {}", msg)),
        }
    }

    /// `/sys`: hardware / platform diagnostics.
    fn cmd_sys(&mut self) {
        let arch = env::consts::ARCH;
        let os_family = env::consts::OS;
        let cores = num_cores();
        let host = env::var("COMPUTERNAME")
            .or_else(|_| env::var("HOSTNAME"))
            .unwrap_or_else(|_| "unknown-host".to_string());
        self.set_results(vec![
            format!("[SYS] Architecture : {}", arch),
            format!("[SYS] OS family      : {}", os_family),
            format!("[SYS] CPU cores      : {}", cores),
            format!("[SYS] Host           : {}", host),
            format!("[SYS] Working dir    : {}", cwd_display()),
            format!("[SYS] Build          : stripped, LTO, opt-level=3"),
        ]);
        self.set_status(format!("{} {} | {} core(s)", os_family, arch, cores));
    }

    // ---- Smart intent cascade (input without '/') ---------------------------

    fn smart_cascade(&mut self, raw: &str) {
        // 1) Try arithmetic evaluation (only if it looks like math).
        if looks_like_math(raw) {
            if let Ok(value) = eval_expr(raw) {
                let pretty = format_number(value);
                self.set_results(vec![format!("{} = {}", raw, pretty)]);
                self.set_status("Routed: arithmetic.");
                return;
            }
        }
        // 2) Existing file/folder on disk? Open it natively.
        let path = expand_tilde(raw);
        if path.exists() {
            match open::that(&path) {
                Ok(()) => {
                    self.set_status(format!("Routed: opened path {}", path.display()));
                    return;
                }
                Err(e) => {
                    self.set_status(format!("Path open failed: {} -- falling back to search", e));
                }
            }
        }
        // 3) Fallback: web search.
        let before = self.status.clone();
        self.cmd_web(raw);
        if !self.status.starts_with("Web search") {
            // /web itself reported a usage/error; keep that message.
            let _ = before;
            return;
        }
        let msg = self.status.clone();
        self.set_status(format!("Routed: {}", msg));
    }
}

// -----------------------------------------------------------------------------
// Detached spawning (platform-specific shell dispatch)
// -----------------------------------------------------------------------------
fn spawn_detached(cmd_line: &str) -> io::Result<()> {
    #[cfg(windows)]
    {
        // `cmd /C start "" <cmd>` spawns a detached process through the shell.
        let mut c = Command::new("cmd");
        c.args(["/C", "start", "", cmd_line])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let _child = c.spawn()?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        // Fire-and-forget detachment: sh -c '<cmd> >/dev/null 2>&1 &'
        let script = format!("{} >/dev/null 2>&1 &", cmd_line);
        let mut c = Command::new("sh");
        c.arg("-c")
            .arg(&script)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let _child = c.spawn()?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Utilities
// -----------------------------------------------------------------------------
fn num_cores() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(0)
}

fn cwd_display() -> String {
    env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "?".to_string())
}

/// Strip surrounding quotes from an argument (handles "..." and '...').
fn shlex_unquote(s: &str) -> String {
    let s = s.trim();
    let bytes = s.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        return s[1..s.len() - 1].to_string();
    }
    s.to_string()
}

/// Expand a leading "~" to the user home directory (cross-platform best effort).
fn expand_tilde(input: &str) -> PathBuf {
    let unquoted = shlex_unquote(input);
    if unquoted == "~" || unquoted.starts_with("~/") || unquoted.starts_with("~\\") {
        let home = env::var("HOME")
            .or_else(|_| env::var("USERPROFILE"))
            .unwrap_or_default();
        if !home.is_empty() {
            let rest = &unquoted[1..];
            let rest = rest.trim_start_matches('/').trim_start_matches('\\');
            return if rest.is_empty() {
                PathBuf::from(home)
            } else {
                PathBuf::from(home).join(rest)
            };
        }
    }
    PathBuf::from(unquoted)
}

/// Percent-encode a query string for URLs (minimal, dependency-free).
fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for byte in input.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{:02X}", other)),
        }
    }
    out
}

/// Heuristic: does this string look like a pure arithmetic expression?
fn looks_like_math(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return false;
    }
    let has_digit = trimmed.chars().any(|c| c.is_ascii_digit());
    let has_op = trimmed
        .chars()
        .any(|c| matches!(c, '+' | '-' | '*' | '/' | '(' | ')'));
    let only_math_chars = trimmed
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '.' | '+' | '-' | '*' | '/' | '(' | ')' | ' '));
    has_digit && has_op && only_math_chars
}

fn format_number(v: f64) -> String {
    if v.is_finite() && v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{:.10}", v)
    }
}

// -----------------------------------------------------------------------------
// Zero-dependency recursive-descent arithmetic parser
// Grammar:
//   expr   := term (('+' | '-') term)*
//   term   := factor (('*' | '/') factor)*
//   factor := ('+' | '-') factor | '(' expr ')' | number
// -----------------------------------------------------------------------------
struct Parser<'a> {
    chars: Vec<char>,
    pos: usize,
    #[allow(dead_code)]
    src: &'a str,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self { chars: src.chars().collect(), pos: 0, src }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn parse_expr(&mut self) -> Result<f64, String> {
        let mut value = self.parse_term()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some('+') => {
                    self.bump();
                    value += self.parse_term()?;
                }
                Some('-') => {
                    self.bump();
                    value -= self.parse_term()?;
                }
                _ => break,
            }
        }
        Ok(value)
    }

    fn parse_term(&mut self) -> Result<f64, String> {
        let mut value = self.parse_factor()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some('*') => {
                    self.bump();
                    value *= self.parse_factor()?;
                }
                Some('/') => {
                    self.bump();
                    let divisor = self.parse_factor()?;
                    if divisor == 0.0 {
                        return Err("division by zero".to_string());
                    }
                    value /= divisor;
                }
                _ => break,
            }
        }
        Ok(value)
    }

    fn parse_factor(&mut self) -> Result<f64, String> {
        self.skip_ws();
        match self.peek() {
            Some('+') => {
                self.bump();
                self.parse_factor()
            }
            Some('-') => {
                self.bump();
                Ok(-self.parse_factor()?)
            }
            Some('(') => {
                self.bump();
                let value = self.parse_expr()?;
                self.skip_ws();
                if self.peek() != Some(')') {
                    return Err("missing closing parenthesis".to_string());
                }
                self.bump();
                Ok(value)
            }
            Some(c) if c.is_ascii_digit() || c == '.' => self.parse_number(),
            _ => Err(format!("unexpected token at position {}", self.pos)),
        }
    }

    fn parse_number(&mut self) -> Result<f64, String> {
        self.skip_ws();
        let start = self.pos;
        let mut seen_dot = false;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.bump();
            } else if c == '.' && !seen_dot {
                seen_dot = true;
                self.bump();
            } else {
                break;
            }
        }
        let token: String = self.chars[start..self.pos].iter().collect();
        token
            .parse::<f64>()
            .map_err(|_| format!("invalid number near \"{}\"", token))
    }
}

/// Evaluate an arithmetic string; errors are returned as messages, never panics.
fn eval_expr(src: &str) -> Result<f64, String> {
    if src.trim().is_empty() {
        return Err("empty expression".to_string());
    }
    let mut parser = Parser::new(src);
    let value = parser.parse_expr()?;
    parser.skip_ws();
    if parser.peek().is_some() {
        let trailing: String = parser.chars[parser.pos..].iter().collect();
        return Err(format!("trailing characters: \"{}\"", trailing.trim()));
    }
    if !value.is_finite() {
        return Err("result is not finite".to_string());
    }
    Ok(value)
}

// -----------------------------------------------------------------------------
// TUI rendering
// -----------------------------------------------------------------------------
fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // top input bar
            Constraint::Min(1),    // results view
            Constraint::Length(1), // bottom status strip
        ])
        .split(area);

    draw_input(f, app, chunks[0]);
    draw_results(f, app, chunks[1]);
    draw_status(f, app, chunks[2]);
}

fn draw_input(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(format!(" {} :: {} ", APP_NAME, APP_TAGLINE))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));

    let line = Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::styled(app.input.clone(), Style::default().fg(Color::White)),
        Span::styled("█", Style::default().fg(Color::DarkGray)),
    ]);
    let para = Paragraph::new(line).block(block);
    f.render_widget(para, area);
}

fn draw_results(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .results
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let style = if i == app.selected {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else if text.starts_with("[DIR]") {
                Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)
            } else if text.starts_with("[FILE]") {
                Style::default().fg(Color::Gray)
            } else if text.starts_with("[SYS]") {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(Line::from(Span::styled(text.clone(), style)))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(" Results ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    let mut state = ListState::default();
    if !app.results.is_empty() {
        state.select(Some(app.selected));
    }
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let is_error = app.status.contains("failed")
        || app.status.contains("error")
        || app.status.contains("Error")
        || app.status.contains("not found");
    let color = if is_error { Color::Red } else { Color::Green };
    let para = Paragraph::new(Line::from(Span::styled(
        format!(" {} | Up/Down navigate | Enter execute | Esc quit", app.status),
        Style::default().fg(color),
    )));
    f.render_widget(para, area);
}

// -----------------------------------------------------------------------------
// Event loop
// -----------------------------------------------------------------------------
fn run(
    terminal: &mut Terminal<ratatui::backend::CrosstermBackend<Stdout>>,
) -> io::Result<()> {
    let mut app = App::new();
    loop {
        terminal.draw(|f| draw(f, &app))?;
        // Non-blocking poll: keeps the UI responsive even when no input is
        // arriving (and lets us detect a closed/EOF stdin instead of spinning).
        if !event::poll(std::time::Duration::from_millis(100))? {
            if is_stdin_eof() {
                return Ok(());
            }
            continue;
        }
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            // Ctrl+C also quits cleanly.
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                app.should_quit = true;
            }
            match key.code {
                KeyCode::Esc => app.should_quit = true,
                KeyCode::Enter => app.submit(),
                KeyCode::Up => app.move_up(),
                KeyCode::Down => app.move_down(),
                KeyCode::Backspace => {
                    app.input.pop();
                }
                KeyCode::Char(c) => {
                    app.input.push(c);
                }
                _ => {}
            }
        }
        if app.should_quit {
            return Ok(());
        }
    }
}

/// Best-effort detection of EOF on stdin (used only when stdin is not a TTY,
/// e.g. piped input in automated tests), so the app can't spin forever.
#[cfg(unix)]
fn is_stdin_eof() -> bool {
    use std::os::unix::io::AsRawFd;
    let fd = std::io::stdin().as_raw_fd();
    let mut buf = [0u8; 1];
    // Peek without consuming; 0 means EOF, -1 with EAGAIN means data pending.
    let n = unsafe { libc_recv(fd, buf.as_mut_ptr(), 1, MSG_PEEK_FLAGS) };
    if n < 0 {
        return false; // would-block or error: treat as "not EOF"
    }
    n == 0
}

#[cfg(unix)]
const MSG_PEEK_FLAGS: i32 = 2; // MSG_PEEK

#[cfg(unix)]
extern "C" {
    #[link_name = "recv"]
    fn libc_recv(fd: i32, buf: *mut u8, len: usize, flags: i32) -> isize;
}

#[cfg(not(unix))]
fn is_stdin_eof() -> bool {
    false
}

// -----------------------------------------------------------------------------
// Entry point: terminal setup / teardown with guaranteed restoration
// -----------------------------------------------------------------------------
fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal);

    // Always restore the terminal, even on internal errors.
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_arithmetic() {
        assert_eq!(eval_expr("2+3").unwrap(), 5.0);
        assert_eq!(eval_expr("10-4").unwrap(), 6.0);
        assert_eq!(eval_expr("6*7").unwrap(), 42.0);
        assert_eq!(eval_expr("81/9").unwrap(), 9.0);
    }

    #[test]
    fn test_precedence_and_parens() {
        assert_eq!(eval_expr("2+3*4").unwrap(), 14.0);
        assert_eq!(eval_expr("(2+3)*4").unwrap(), 20.0);
        assert_eq!(eval_expr("(2+3)*4/2").unwrap(), 10.0);
        assert_eq!(eval_expr("-5+2").unwrap(), -3.0);
        assert_eq!(eval_expr("2.5*4").unwrap(), 10.0);
    }

    #[test]
    fn test_errors() {
        assert!(eval_expr("2+/").is_err());
        assert!(eval_expr("(2+3").is_err());
        assert!(eval_expr("1/0").is_err());
        assert!(eval_expr("abc").is_err());
        assert!(eval_expr("").is_err());
        assert!(eval_expr("2 3").is_err());
    }

    #[test]
    fn test_url_encode() {
        assert_eq!(url_encode("hello world"), "hello+world");
        assert_eq!(url_encode("a&b"), "a%26b");
    }

    #[test]
    fn test_looks_like_math() {
        assert!(looks_like_math("2+2"));
        assert!(looks_like_math("(3)*4"));
        assert!(!looks_like_math("report.pdf"));
        assert!(!looks_like_math("hello"));
    }

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(10.0), "10");
        assert_eq!(format_number(2.5), "2.5000000000");
    }
}

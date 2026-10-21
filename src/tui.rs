//! ============================================================================
//! CLDR — Command Line Dispatch & Route
//! © 2026 Dr. Sohil Momin (drsamonline). All rights reserved.
//! MIT-licensed: THE ABOVE COPYRIGHT / ATTRIBUTION NOTICE MUST BE RETAINED IN
//! ALL COPIES OR SUBSTANTIAL PORTIONS OF THIS SOFTWARE. Removing this
//! watermark is a violation of the license terms.
//! ============================================================================
//!
//! Interactive terminal UI: raw-mode crossterm + ratatui.
//!
//! Performance notes:
//!   * The screen is redrawn only when something actually changed (`dirty`
//!     flag). The idle loop therefore costs one `poll()` every 250 ms and
//!     nothing else — no 60 fps busy redraw.
//!   * The results pane is a bounded `Vec` with a hard scrollback cap, so
//!     memory cannot grow without bound.

use std::cell::RefCell;
use std::env;
use std::io::{self, Stdout, Write};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
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

use crate::engine::{execute_input, push, Entry, APP_NAME, APP_TAGLINE, AUTHOR};
use crate::extras;

/// Event poll interval. Events wake the loop immediately, so this only governs
/// how often we notice the cursor-blink tick — no input latency is added.
const POLL_MS: u64 = 250;
const HISTORY_CAP: usize = 500;

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

pub struct App {
    input: String,
    cursor_visible: bool,
    entries: Rc<RefCell<Vec<Entry>>>,
    list_state: ListState,
    status: String,
    running: bool,
    dirty: bool,
    cwd: PathBuf,
    history: Vec<String>,
    hist_idx: Option<usize>,
    draft: String,
    /// Set by the summon watcher thread when the tray/hotkey asks us to come forward.
    pub summoned: Arc<AtomicBool>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
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
        push(
            &entries,
            Entry::new("INFO", "type /help for commands · ↑ history · Ctrl+Y copy · Esc quit"),
        );
        let mut app = Self {
            input: String::new(),
            cursor_visible: true,
            entries,
            list_state: ListState::default(),
            status: "ready".into(),
            running: true,
            dirty: true,
            cwd: env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            history: extras::load_history(),
            hist_idx: None,
            draft: String::new(),
            summoned: Arc::new(AtomicBool::new(false)),
        };
        app.jump_bottom();
        app
    }

    /// Headless entry used by `cldr --exec "<input>"` (shortcut one-shot runs).
    pub fn dispatch(raw: &str) -> String {
        let entries = Rc::new(RefCell::new(Vec::<Entry>::new()));
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let status = execute_input(raw, &cwd, &entries);
        for e in entries.borrow().iter() {
            println!("[{}] {}", e.kind, e.text);
        }
        status
    }

    fn submit(&mut self) {
        let raw = std::mem::take(&mut self.input);
        self.hist_idx = None;
        self.draft.clear();
        let t = raw.trim().to_string();
        if !t.is_empty() && self.history.last().map(String::as_str) != Some(t.as_str()) {
            self.history.push(t.clone());
            if self.history.len() > HISTORY_CAP {
                self.history.drain(..self.history.len() - HISTORY_CAP);
            }
            extras::append_history(&t);
        }
        self.status = execute_input(&raw, &self.cwd, &self.entries);
        self.jump_bottom();
        self.dirty = true;
    }

    fn hist_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let idx = match self.hist_idx {
            None => {
                self.draft = self.input.clone();
                self.history.len() - 1
            }
            Some(0) => 0,
            Some(i) => i - 1,
        };
        self.hist_idx = Some(idx);
        self.input = self.history[idx].clone();
        self.dirty = true;
    }

    fn hist_next(&mut self) {
        match self.hist_idx {
            None => {}
            Some(i) if i + 1 < self.history.len() => {
                self.hist_idx = Some(i + 1);
                self.input = self.history[i + 1].clone();
            }
            Some(_) => {
                self.hist_idx = None;
                self.input = std::mem::take(&mut self.draft);
            }
        }
        self.dirty = true;
    }

    /// Copy the newest result (or the status line) to the terminal clipboard
    /// with OSC 52 — no clipboard crate, works over SSH too.
    fn copy_last(&mut self) {
        let text = self
            .entries
            .borrow()
            .last()
            .map(|e| e.text.clone())
            .unwrap_or_else(|| self.status.clone());
        if text.is_empty() {
            return;
        }
        let b64 = extras::base64(text.as_bytes());
        let mut out = io::stdout();
        let _ = write!(out, "\x1b]52;c;{}\x07", b64);
        let _ = out.flush();
        self.status = format!("copied: {}", truncate(&text, 40));
        self.dirty = true;
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

    fn on_key(&mut self, code: KeyCode, mods: KeyModifiers) {
        use KeyCode::*;
        let ctrl = mods.contains(KeyModifiers::CONTROL);
        let shift = mods.contains(KeyModifiers::SHIFT);

        if ctrl {
            match code {
                Char('c') | Char('C') => {
                    self.running = false;
                    return;
                }
                Char('y') | Char('Y') => {
                    self.copy_last();
                    return;
                }
                Char('p') | Char('P') => {
                    self.hist_prev();
                    return;
                }
                Char('n') | Char('N') => {
                    self.hist_next();
                    return;
                }
                Char('l') | Char('L') => {
                    self.entries.borrow_mut().clear();
                    self.dirty = true;
                    return;
                }
                _ => {}
            }
        }

        match code {
            Char(c) => {
                if shift && c.is_alphabetic() {
                    self.input.push(c.to_ascii_uppercase());
                } else {
                    self.input.push(c);
                }
                self.dirty = true;
            }
            Backspace => {
                self.input.pop();
                self.dirty = true;
            }
            Delete => {
                self.input.clear();
                self.dirty = true;
            }
            Enter => self.submit(),
            Up => {
                if self.input.is_empty() {
                    self.hist_prev();
                } else {
                    self.move_sel(-1);
                }
                self.dirty = true;
            }
            Down => {
                if self.input.is_empty() {
                    self.hist_next();
                } else {
                    self.move_sel(1);
                }
                self.dirty = true;
            }
            PageUp => {
                self.move_sel(-10);
                self.dirty = true;
            }
            PageDown => {
                self.move_sel(10);
                self.dirty = true;
            }
            Home if !self.entries.borrow().is_empty() => {
                self.list_state.select(Some(0));
                self.dirty = true;
            }
            End => {
                self.jump_bottom();
                self.dirty = true;
            }
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
        let cursor = if self.cursor_visible { "▏" } else { " " };
        let para = Paragraph::new(Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(self.input.clone(), Style::default().fg(Color::White)),
            Span::styled(cursor.to_string(), Style::default().fg(Color::Yellow)),
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
// Compact window (Windows): shrink the console to a small, focused box.
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn compact_window() {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetConsoleWindow() -> *mut c_void;
        fn SetConsoleScreenBufferSize(handle: *mut c_void, size: u32) -> i32;
        fn SetConsoleWindowInfo(handle: *mut c_void, absolute: i32, rect: *const i16) -> i32;
    }
    #[link(name = "user32")]
    extern "system" {
        fn MoveWindow(hwnd: *mut c_void, x: i32, y: i32, w: i32, h: i32, repaint: i32) -> i32;
        fn GetSystemMetrics(index: i32) -> i32;
    }

    const COLS: i16 = 92;
    const ROWS: i16 = 26;

    unsafe {
        let h = GetConsoleWindow();
        if h.is_null() {
            return;
        }
        // The buffer must be >= the window before the window can be shrunk.
        let buffer = (COLS as u32) | ((ROWS as u32) << 16);
        SetConsoleScreenBufferSize(h, buffer);
        let rect: [i16; 4] = [0, 0, COLS - 1, ROWS - 1];
        SetConsoleWindowInfo(h, 1, rect.as_ptr());

        // Roughly centre it horizontally, a little above the middle.
        let sw = GetSystemMetrics(0); // SM_CXSCREEN
        let sh = GetSystemMetrics(1); // SM_CYSCREEN
        let cw = (COLS as i32) * 9;
        let ch = (ROWS as i32) * 19;
        MoveWindow(h, (sw - cw) / 2, sh / 7, cw, ch, 1);
    }
}

// ---------------------------------------------------------------------------
// Terminal lifecycle
// ---------------------------------------------------------------------------

/// Restore the real terminal: leave alternate screen, disable raw mode/mouse,
/// show cursor again. Safe to call multiple times.
pub fn cleanup() {
    let _ = disable_raw_mode();
    let mut stdout = io::stdout();
    let _ = execute!(stdout, LeaveAlternateScreen, DisableMouseCapture);
    let _ = stdout.write_all(b"\x1b[?25h");
    let _ = stdout.flush();
}

fn attach_terminal() -> io::Result<CrosstermBackend<Stdout>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    Ok(CrosstermBackend::new(stdout))
}

pub fn run_tui() -> io::Result<()> {
    #[cfg(windows)]
    compact_window();

    let backend = attach_terminal()?;
    let mut terminal = ratatui::Terminal::new(backend)?;
    let _ = terminal.hide_cursor();
    let result = event_loop(&mut terminal);
    cleanup();
    result
}

fn event_loop(terminal: &mut ratatui::Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    let mut app = App::new();

    // Cursor-blink ticker: a single detached thread, ~0 CPU.
    let blink = Arc::new(AtomicBool::new(false));
    let b = Arc::clone(&blink);
    let blink_handle: JoinHandle<()> = thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(500));
        b.store(true, Ordering::Relaxed);
    });

    // A1: loopback IPC control channel — `cldr --summon`, tray clicks and
    // hotkeys connect here; SUMMON raises our window and wakes the redraw.
    crate::ipc::declare_window_instance();
    let _ipc_handle = crate::ipc::spawn_server(&app.summoned);

    terminal.draw(|f| app.draw(f))?;
    app.dirty = false;

    while app.running {
        if event::poll(Duration::from_millis(POLL_MS))? {
            match event::read()? {
                Event::Key(k) if k.kind == KeyEventKind::Press => app.on_key(k.code, k.modifiers),
                Event::Resize(_, _) => app.dirty = true,
                _ => {}
            }
        }
        if app.summoned.swap(false, Ordering::Relaxed) {
            app.status = "summoned from tray".into();
            app.dirty = true;
        }
        if blink.swap(false, Ordering::Relaxed) {
            app.cursor_visible = !app.cursor_visible;
            app.dirty = true;
        }
        // Redraw only on change: idle cost is a single poll() per 250 ms.
        if app.dirty {
            terminal.draw(|f| app.draw(f))?;
            app.dirty = false;
        }
    }

    drop(blink_handle);
    // A1: deregister the loopback rendezvous so future launches open a fresh
    // window instead of summoning this dead one.
    crate::ipc::unregister();
    Ok(())
}

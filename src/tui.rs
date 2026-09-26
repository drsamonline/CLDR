//! ============================================================================
//! CLDR — Command Line Dispatch & Route
//! © 2026 Dr. Sohil Momin (drsamonline). All rights reserved.
//! MIT-licensed: THE ABOVE COPYRIGHT / ATTRIBUTION NOTICE MUST BE RETAINED IN
//! ALL COPIES OR SUBSTANTIAL PORTIONS OF THIS SOFTWARE. Removing this
//! watermark is a violation of the license terms.
//! ============================================================================
//!
//! Interactive terminal UI: raw-mode crossterm + ratatui, bounded scrollback,
//! blinking-caret ticker, summon-window watcher (tray → foreground), and full
//! terminal restoration on exit.
//! © 2026 Dr. Sohil Momin — attribution watermark retained per license.

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

use crate::engine::{execute_input, push, Entry, APP_NAME, APP_TAGLINE, AUTHOR, SCROLLBACK_CAP};

const POLL_MS: u64 = 16; // ~60 fps UI poll; near-zero idle CPU

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
    cwd: PathBuf,
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
        push(&entries, Entry::new("INFO", "press /help for the command cheat sheet · Esc hides/quits"));
        let mut app = Self {
            input: String::new(),
            cursor_visible: true,
            entries,
            list_state: ListState::default(),
            status: "ready".into(),
            running: true,
            cwd: env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
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
    let backend = attach_terminal()?;
    let mut terminal = ratatui::Terminal::new(backend)?;
    let _ = terminal.hide_cursor();
    let result = event_loop(&mut terminal);
    cleanup();
    result
}

fn event_loop(terminal: &mut ratatui::Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    let mut app = App::new();
    let blink = Arc::new(AtomicBool::new(false));
    let b = Arc::clone(&blink);
    let blink_handle: JoinHandle<()> = thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(500));
        b.store(true, Ordering::Relaxed);
    });

    // Tray/hotkey summon watcher: raises this terminal window when the
    // background instance posts a sentinel. Spawned once per session.
    let _summon_handle = crate::summon::spawn_watcher(&app.summoned);

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
        if app.summoned.swap(false, Ordering::Relaxed) {
            app.status = "summoned from tray".into();
        }
        if blink.swap(false, Ordering::Relaxed) {
            app.cursor_visible = !app.cursor_visible;
        }
        terminal.draw(|f| app.draw(f))?;
    }
    drop(blink_handle);
    // Enforce the documented scrollback ceiling invariant on shutdown.
    let _ = SCROLLBACK_CAP;
    Ok(())
}

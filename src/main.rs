use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, Event as CEvent, KeyCode},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

#[derive(Debug)]
pub struct State {
    is_running: bool,
    mode: Mode,
    status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
}

impl State {
    pub fn new() -> Self {
        Self {
            is_running: true,
            mode: Mode::Normal,
            status: "Welcome to dbvi! Press `q` to quit.".into(),
        }
    }

    fn handle_input(&mut self, event: CEvent) {
        if let CEvent::Key(key) = event {
            match self.mode {
                Mode::Normal => match key.code {
                    KeyCode::Char('q') => self.is_running = false,
                    KeyCode::Char('i') => self.mode = Mode::Insert,
                    _ => {}
                },
                Mode::Insert => match key.code {
                    KeyCode::Esc => self.mode = Mode::Normal,
                    _ => {}
                },
            }
        }
    }
}

fn draw_ui(f: &mut ratatui::Frame, state: &State) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Min(5),    // body
            Constraint::Length(2), // footer command input
        ])
        .split(f.area());

    let body = Paragraph::new("Query results will go here...")
        .block(
            Block::default()
                .title(Line::from("Results").centered())
                .borders(Borders::TOP),
        )
        .style(Style::default().fg(Color::White));
    f.render_widget(body, chunks[0]);

    let footer = Paragraph::new(format!("Mode: {:?} | {}", state.mode, state.status))
        .block(Block::default().borders(Borders::TOP));
    f.render_widget(footer, chunks[1]);
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mut state: State,
) -> io::Result<()> {
    while state.is_running {
        terminal.draw(|f| draw_ui(f, &state))?;

        if event::poll(Duration::from_millis(200))? {
            let ev = event::read()?;
            state.handle_input(ev);
        }
    }
    Ok(())
}

pub struct App {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl App {
    pub fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }

    pub fn run(mut self) -> io::Result<()> {
        let state = State::new();
        run_app(&mut self.terminal, state)
    }
}

impl Drop for App {
    fn drop(&mut self) {
        disable_raw_mode().expect("Could not disable raw mode");
        execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)
            .expect("Could not leave alternate screen");
        self.terminal.show_cursor().expect("Could not show cursor");
    }
}

fn main() -> io::Result<()> {
    App::new()?.run()
}

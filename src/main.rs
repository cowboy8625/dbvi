// Copyright 2025 cowboy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use clap::Parser;
use std::time::Duration;
use std::{io, pin::Pin};

use crossterm::{
    cursor::Show,
    event::{self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode},
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
use sqlx::PgPool;

#[derive(Debug)]
pub struct State {
    is_running: bool,
    mode: Mode,
    status: String,
    query: String,
    pool: PgPool,
    result: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    RunQuery(String),
    Chain(Vec<Command>),
    None,
    Quit,
}

impl State {
    pub fn new(pool: PgPool) -> Self {
        Self {
            is_running: true,
            mode: Mode::Normal,
            status: "Welcome to dbvi! Press `q` to quit.".into(),
            query: String::new(),
            result: String::new(),
            pool,
        }
    }
}

fn handle_input(state: &mut State, event: CEvent) -> Command {
    let CEvent::Key(key) = event else {
        return Command::None;
    };

    let mode = state.mode;
    match mode {
        Mode::Normal => match key.code {
            KeyCode::Char('q') => Command::Quit,
            KeyCode::Char('i') => {
                state.mode = Mode::Insert;
                Command::None
            }
            _ => Command::None,
        },
        Mode::Insert => match key.code {
            KeyCode::Esc => {
                state.mode = Mode::Normal;
                Command::None
            }
            KeyCode::Char(c) => {
                state.query.push(c);
                Command::None
            }
            KeyCode::Enter => {
                state.mode = Mode::Normal;
                Command::RunQuery(state.query.clone())
            }
            KeyCode::Backspace => {
                // TODO: once we make the cursor moveable we will need to account for that here.
                // So pressing i put you in Insert mode but really that is insert for the
                // query mode and then if we want app commands :
                // Probably obviouse.
                state.query.pop();
                Command::None
            }
            _ => Command::None,
        },
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

    let query_result = if state.result.is_empty() {
        "Query results will go here..."
    } else {
        &state.result
    };
    let body = Paragraph::new(query_result)
        .block(
            Block::default()
                .title(Line::from("Results").centered())
                .borders(Borders::TOP),
        )
        .style(Style::default().fg(Color::White));
    f.render_widget(body, chunks[0]);

    let footer_text = format!("> {}", state.query);
    let footer_title = Line::from(format!("Mode: {:?} | {}", state.mode, state.status));
    let footer = Paragraph::new(footer_text)
        .block(Block::default().title(footer_title).borders(Borders::TOP));
    if state.mode == Mode::Insert {
        // Cursor X: after "> " 2 + 1 so it will be on the right side
        let cursor_x = 3 + state.query.len() as u16;
        // Cursor Y: top line of footer chunk
        let cursor_y = chunks[1].y + 1; // +1 for the border
        f.set_cursor_position((cursor_x, cursor_y));
    }
    f.render_widget(footer, chunks[1]);
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mut state: State,
) -> io::Result<()> {
    while state.is_running {
        terminal.draw(|f| draw_ui(f, &state))?;

        if !event::poll(Duration::from_millis(200))? {
            continue;
        }

        let ev = event::read()?;
        let cmd = handle_input(&mut state, ev);
        handle_command(cmd, &mut state, terminal).await?;
    }
    Ok(())
}

fn handle_command<'a>(
    cmd: Command,
    state: &'a mut State,
    terminal: &'a mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Pin<Box<dyn Future<Output = io::Result<()>> + 'a>> {
    Box::pin(async move {
        match cmd {
            Command::RunQuery(raw_query) => {
                match sqlx::query(&raw_query).fetch_all(&state.pool).await {
                    Ok(results) => {
                        state.result = format!("{:?}", results);
                        state.status = "Query executed successfully".into();
                        state.query.clear();
                    }
                    Err(err) => {
                        state.result = "".into();
                        state.status = format!("Failed to run query: {}", err);
                    }
                }
            }
            Command::Quit => state.is_running = false,
            Command::None => {}
            Command::Chain(cmds) => {
                for cmd in cmds {
                    handle_command(cmd, state, terminal).await?;
                }
            }
        }
        Ok(())
    })
}

pub struct App {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    pool: PgPool,
}

impl App {
    pub async fn new(args: &Args) -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        let Some(url) = args.url.as_ref() else {
            // TODO: Maybe have a toast warning the user that the database is not connected
            restore_terminal_state()?;
            return Err(io::Error::new(io::ErrorKind::Other, "Missing database URL"));
        };
        let pool = PgPool::connect(url)
            .await
            .expect("Failed to connect to database");

        Ok(Self { terminal, pool })
    }

    pub async fn run(mut self) -> io::Result<()> {
        let state = State::new(self.pool.clone());
        run_app(&mut self.terminal, state).await
    }
}

#[inline(always)]
fn restore_terminal_state() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(
        io::stdout(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        Show
    )?;
    Ok(())
}

impl Drop for App {
    fn drop(&mut self) {
        restore_terminal_state().expect("Failed to restore terminal state");
    }
}

#[derive(clap::Parser)]
pub struct Args {
    #[clap(short, long)]
    pub url: Option<String>,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let args = Args::parse();
    App::new(&args).await?.run().await
}

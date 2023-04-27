use clap::*;
use color_eyre::Report;
use crossterm::event::read;
use crossterm::style::Stylize;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::fs;
use std::io::{self, stdout};
use tracing_subscriber::fmt::Subscriber;
use tracing_subscriber::FmtSubscriber;
use tui::style::{Color, Modifier, Style};
use tui::widgets::{List, ListItem, ListState, Paragraph};
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, Widget},
    Frame, Terminal,
};

#[derive(Parser, Debug)]
#[command(name = "Sxredder")]
#[command(author = "foom")]
#[command(version = "0.1")]
#[command(
    about = "Shreds files digitally",
    long_about = "Rewrites files with random data before writing all x to it, then deleting it"
)]
struct Opts {
    ///Enable verbose output
    #[arg(short)]
    verbose: bool,
}

struct FileList<T> {
    state: ListState,
    files: Vec<T>,
    file_paths: Vec<String>,
}

impl<T> FileList<T> {
    fn new(files: Vec<T>, file_paths: Vec<String>) -> FileList<T> {
        let mut state = ListState::default();
        state.select(Some(0));
        FileList {
            state,
            files,
            file_paths,
        }
    }

    fn move_down(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.files.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        if !self.files.is_empty() {
            self.state.select(Some(i));
        }

    }

    fn move_up(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.files.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        if !self.files.is_empty() {
            self.state.select(Some(i));
        }
    }
}

fn main() -> Result<(), Report> {
    let opts = Opts::parse();
    let subscriber = init_logging(opts);

    tracing::subscriber::set_global_default(subscriber).expect("setting tracing default failed");
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // terminal.clear()?;
    // terminal.hide_cursor()?;

    let items = read_current_dir();
    let mut file_list = FileList::new(
        items
            .iter()
            .map(|i| ListItem::new(i.as_str()))
            .collect::<Vec<ListItem>>(),
        items.clone(),
    );

    let draw_panels = |frame: &mut Frame<CrosstermBackend<io::Stdout>>,
                       file_list: &mut FileList<ListItem>| {
        let main_layout = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Percentage(90), Constraint::Length(3)].as_ref())
            .split(frame.size());
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .margin(1)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
            .split(main_layout[0]);

        let left_block = List::new(file_list.files.clone())
            .block(Block::default().borders(Borders::ALL).title("Dir"))
            .style(Style::default().fg(Color::Cyan))
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::ITALIC)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("->");
        let right_block = Block::default()
            .title("Panel 2")
            .borders(tui::widgets::Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));

        frame.render_stateful_widget(left_block, chunks[0], &mut file_list.state);
        frame.render_widget(right_block, chunks[1]);

        // Mnemonic keys panel
        let mnemonic_keys_text = "[Q]uit";
        let mnemonic_keys = Paragraph::new(mnemonic_keys_text)
            .style(Style::default().fg(Color::Green))
            .block(
                Block::default()
                    .title("Mnemonic Keys")
                    .borders(tui::widgets::Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow)),
            );
        frame.render_widget(mnemonic_keys, main_layout[1]);
    };

    loop {
        terminal.draw(|f| draw_panels(f, &mut file_list))?;
        if let Event::Key(event) = read()? {
            match event.code {
                KeyCode::Char('q') => break,
                KeyCode::Up | KeyCode::Char('k') => file_list.move_up(),
                KeyCode::Down | KeyCode::Char('j') => file_list.move_down(),
                _ => continue,
            }
        }
    }

    disable_raw_mode()?;

    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;

    terminal.clear()?;
    terminal.show_cursor()?;
    Ok(())
}

fn init_logging(opts: Opts) -> Subscriber {
    let verbose = opts.verbose;
    // Initialize the logging system based on the verbose flag
    let subscriber = FmtSubscriber::builder()
        .with_max_level(if opts.verbose {
            tracing::Level::INFO
        } else {
            tracing::Level::WARN
        })
        .finish();
    subscriber
}

fn read_current_dir() -> Vec<String> {
    let mut items = Vec::new();
    if let Ok(entries) = fs::read_dir(".") {
        for entry in entries {
            if let Ok(entry) = entry {
                if let Some(file_name) = entry.file_name().to_str() {
                    items.push(file_name.to_string());
                }
            }
        }
    }
    items
}

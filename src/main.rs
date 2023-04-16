use clap::*;
use color_eyre::Report;
use crossterm::event::read;
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
use tui::widgets::{List, ListItem};
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

    let draw_panels = |frame: &mut Frame<CrosstermBackend<io::Stdout>>| {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .margin(1)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
            .split(frame.size());

        let items = read_current_dir();
        let items = items
            .iter()
            .map(|i| ListItem::new(i.as_str()))
            .collect::<Vec<ListItem>>();
        let left_block = List::new(items).block(
            Block::default().borders(Borders::ALL).title("Dir"))
                                         .style(Style::default().fg(Color::White))
                                         .highlight_style(Style::default().add_modifier(Modifier::ITALIC).add_modifier(Modifier::BOLD))
                                         .highlight_symbol("->");
        let right_block = Block::default()
            .title("Panel 2")
            .borders(tui::widgets::Borders::ALL);

        frame.render_widget(left_block, chunks[0]);
        frame.render_widget(right_block, chunks[1]);


    };

    loop {
        terminal.draw(draw_panels)?;

        if let Event::Key(event) = read()? {
            match event.code {
                KeyCode::Char('q') => break,
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

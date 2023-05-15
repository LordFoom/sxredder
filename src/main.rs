use clap::*;
use color_eyre::Report;
use crossterm::event::read;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::{fs, io};
use tracing_subscriber::fmt::Subscriber;
use tracing_subscriber::FmtSubscriber;
use tui::layout::Alignment;
use tui::style::{Color, Modifier, Style};
use tui::text::{Span, Spans};
use tui::widgets::{List, ListItem, ListState, Paragraph, Wrap};
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders},
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

#[derive(Clone)]
struct FileList<'a> {
    state: ListState,
    files: Vec<ListItem<'a>>,
    file_paths: Vec<PathBuf>,
}

struct SxredderState<'a> {
    ///where are we in the file system
    current_dir: String,
    ///List of files to display
    file_list: FileList<'a>,
    right_pane_content: Paragraph<'a>,
    ///do we need to update the ui
    changed: bool,
}

impl SxredderState<'_> {
    fn move_down(&mut self) {
        self.file_list.move_down();
        let idx = self.file_list.state.selected().unwrap_or(0);
        self.update_preview_pane_content(idx);
    }

    fn move_up(&mut self) {
        self.file_list.move_up();
        let idx = self.file_list.state.selected().unwrap_or(0);
        self.update_preview_pane_content(idx);
    }

    fn selected_item(&self) -> (ListItem, PathBuf) {
        self.file_list.selected_item()
    }
    ///Create preview on the right hand pane
    /// * Dir shows file list
    /// * file shows head of file
    fn update_preview_pane_content(&mut self, index: usize) {
        if let Some(pb) = self.file_list.file_paths.get(index) {
            let preview = if pb.is_dir() {
                //load the dir
                read_directory(&pb.as_os_str().to_str().unwrap())
                    .unwrap()
                    .into_iter()
                    .map(|pb| {
                        let option = pb.file_name().unwrap().to_str().unwrap().to_string();
                        Spans::from(Span::styled(option, Style::default().fg(Color::Yellow)))
                    })
                    .collect()
            } else {
                //TODO get the first few lines of text
                let file = File::open(pb).unwrap();
                let buff = BufReader::new(file);
                let mut display: Vec<Spans> = buff
                    .lines()
                    .into_iter()
                    .take(10)
                    .map(|res| res.unwrap())
                    .map(|line| {
                        Spans::from(Span::styled(line, Style::default().fg(Color::Yellow)))
                    })
                    .collect();
                display
            };
            self.right_pane_content = Paragraph::new(preview)
                .alignment(Alignment::Left)
                .wrap(Wrap { trim: true });
        }
    }
}

impl FileList<'_> {
    fn new(files: Vec<ListItem>, file_paths: Vec<PathBuf>) -> FileList<'_> {
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
            //change the display on the right
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

    fn selected_item(&self) -> (ListItem, PathBuf) {
        let idx = self.state.selected().unwrap();
        let file = self.files.get(idx).unwrap();
        let file_path = self.file_paths.get(idx).unwrap();
        return (file.clone(), file_path.clone());
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

    let items = read_current_dir()?;

    let fl = FileList::new(
        items
            .iter()
            .map(|i| {
                ListItem::new({
                    //should i be doing this directlyh on the path buff?
                    let suffix = if i.is_dir() { "/" } else { "" };
                    let prefix = i.to_str().unwrap_or("Invalid utf-8 path");
                    let stripped_pfx = if prefix.len() > 2 {
                        prefix.replace("./", "")
                    } else {
                        prefix.to_string()
                    };
                    format!("{}{}", stripped_pfx, suffix)
                })
            })
            .collect::<Vec<ListItem>>(),
        items.clone(),
    );

    let mut state = SxredderState {
        current_dir: ".".to_string(),
        file_list: fl.clone(),
        right_pane_content: Paragraph::new(""),
        changed: false,
    };
    state.update_preview_pane_content(0);
    let draw_panels = |frame: &mut Frame<CrosstermBackend<io::Stdout>>,
                       state: &mut SxredderState| {
        let main_layout = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Min(0), Constraint::Length(3)].as_ref())
            .split(frame.size());
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .margin(1)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
            .split(main_layout[0]);

        let panel_colors = Color::Cyan;
        let left_block = List::new(state.file_list.files.clone())
            .block(Block::default().borders(Borders::ALL).title("Dir"))
            .style(Style::default().fg(panel_colors))
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::ITALIC)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("->");

        //if left block is a selected dir, then get the contents of dir and display
        let (_li, pb) = fl.selected_item();
        let is_dir = pb.is_dir();
        // file_list.state.selected()
        //if a file, then show the first few lines (bindary what about that?

        let right_block = Block::default()
            .title("Details")
            .borders(tui::widgets::Borders::ALL)
            .border_style(Style::default().fg(panel_colors));

        let rp_content = state.right_pane_content.clone().block(right_block);
        frame.render_stateful_widget(left_block, chunks[0], &mut state.file_list.state);
        frame.render_widget(rp_content, chunks[1]);

        // Mnemonic keys panel
        let mut mnemonic_keys_text = "[Q]uit".to_string();
        if is_dir {
            mnemonic_keys_text.push_str("|[Enter] dir");
        }
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
        terminal.draw(|f| draw_panels(f, &mut state))?;
        if let Event::Key(event) = read()? {
            match event.code {
                KeyCode::Char('q') => break,
                KeyCode::Up | KeyCode::Char('k') => state.move_up(),
                KeyCode::Down | KeyCode::Char('j') => state.move_down(),
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

fn read_current_dir() -> Result<Vec<PathBuf>, Report> {
    read_directory(".")
}

pub fn read_directory(dir_path: &str) -> Result<Vec<PathBuf>, Report> {
    let entries = fs::read_dir(dir_path)?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect();

    paths.sort_by(|a, b| {
        let a_path = Path::new(a);
        let b_path = Path::new(b);
        // let a_metadata = fs::metadata(&a_path).unwrap();
        // let b_metadata = fs::metadata(&b_path).unwrap();
        match (a.is_dir(), b.is_dir()) {
            (true, true) | (false, false) => a_path.partial_cmp(b_path).unwrap(),
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
        }
    });
    Ok(paths)
}
#[cfg(test)]
mod tests {
    use super::*;

    fn setup_file_list() -> FileList<'static> {
        let files = vec![
            ListItem::new("file1.txt"),
            ListItem::new("file2.txt"),
            ListItem::new("file3.txt"),
        ];
        let file_paths = vec![
            PathBuf::from("file1.txt"),
            PathBuf::from("file2.txt"),
            PathBuf::from("file3.txt"),
        ];
        FileList::new(files, file_paths)
    }

    #[test]
    fn file_list_new() {
        let file_list = setup_file_list();
        assert_eq!(file_list.files.len(), 3);
        assert_eq!(file_list.file_paths.len(), 3);
        assert_eq!(file_list.state.selected(), Some(0));
    }

    #[test]
    fn file_list_move_down() {
        let mut file_list = setup_file_list();
        file_list.move_down();
        assert_eq!(file_list.state.selected(), Some(1));
        file_list.move_down();
        assert_eq!(file_list.state.selected(), Some(2));
        file_list.move_down();
        assert_eq!(file_list.state.selected(), Some(0));
    }

    #[test]
    fn file_list_move_up() {
        let mut file_list = setup_file_list();
        file_list.move_up();
        assert_eq!(file_list.state.selected(), Some(2));
        file_list.move_up();
        assert_eq!(file_list.state.selected(), Some(1));
        file_list.move_up();
        assert_eq!(file_list.state.selected(), Some(0));
    }

    #[test]
    fn file_list_selected_item() {
        let file_list = setup_file_list();
        let (file, file_path) = file_list.selected_item();
        // file.content;
        let file_content = format!("{:?}", file);
        assert!(file_content.contains("file1.txt"));
        assert_eq!(file_path, PathBuf::from("file1.txt"));
    }

    #[test]
    fn read_directory_valid() {
        let paths = read_directory(".").unwrap();
        assert!(!paths.is_empty());
        assert!(paths.iter().all(|path| path.exists()));
    }

    #[test]
    fn read_directory_invalid() {
        let result = read_directory("non_existent_directory");
        assert!(result.is_err());
    }
}

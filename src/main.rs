use clap::*;
use color_eyre::Report;
use crossterm::event::read;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::{fs, io};
use color_eyre::eyre::eyre;
use tracing_subscriber::fmt::Subscriber;
use tracing_subscriber::FmtSubscriber;
use tui::layout::{Alignment, Rect};
use tui::style::{Color, Modifier, Style};
use tui::text::{Span, Spans};
use tui::widgets::canvas::Rectangle;
use tui::widgets::{Clear, List, ListItem, ListState, Paragraph, Wrap};
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders},
    Frame, Terminal,
};
use tui::backend::Backend;

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
    confirm_delete: bool,
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

    // fn update_file_list_pane(&mut self, files: FileList){
    //     self.file_list = files.clone();
    // }
    // fn update_file_list_pane<'a, 'b: 'a>(&'a mut self, files: FileList<'b>){
    //     self.file_list = files;
    // }

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
                let display: Vec<Spans> = buff
                    .lines()
                    .into_iter()
                    .take(100)
                    .map(|res| res.unwrap())
                    .map(|line| Spans::from(Span::styled(line, Style::default().fg(Color::Yellow))))
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
    fn from_paths(file_paths: Vec<PathBuf>) -> Self {
        let items = file_paths
            .iter()
            .map(|i| {
                ListItem::new({
                    //should i be doing this directlyh on the path buff?
                    let suffix = if i.is_dir() { "/" } else { "" };
                    let full_file_path = i.to_str().unwrap_or("Invalid utf-8 path");
                    //now we split the file_path up
                    let just_the_name = i.as_path().file_name().unwrap().to_str().unwrap();
                    format!("{}{}", just_the_name, suffix)
                })
            })
            .collect::<Vec<ListItem>>();

        Self {
            state: Default::default(),
            files: items.clone(),
            file_paths,
        }
    }
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
        let idx = if let Some(idx) = self.state.selected() {
            idx
        } else {
            0
        };
        let file = self.files.get(idx).unwrap();
        let file_path = self.file_paths.get(idx).unwrap();
        (file.clone(), file_path.clone())
    }
}

fn show_warning<B:Backend>(path_buf: &PathBuf, f: &mut Frame<B>) -> Result<(), Report> {
    //divide the screen up into 9 and then draw in the middle
    let txt = path_buf.as_path().file_name().ok_or(eyre!("No os string"))?.to_str().ok_or(eyre!("No path buf"))?;
    let display = Paragraph::new("Really delete? [Y][N]")
        .style(Style::default().fg(Color::Red))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Red))
                .title(txt),
        );

    let area = popup_area(30, 20, f.size());

    f.render_widget(Clear, area);
    f.render_widget(display, area);

    Ok(())
}

fn popup_area(min_horizontal: usize, min_vertical: usize, r: Rect) -> Rect {
    //vertical into three rows
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Min(min_vertical as u16),
            Constraint::Percentage(70),
        ])
        .split(r);

    //now we split the middle one into 3 columns and return the middle one
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Min(min_horizontal as u16),
            Constraint::Percentage(70),
        ])
        .split(v[1])[1]
}

fn sxred_file(path: &Path) -> Result<(), io::Error> {
    // let path = Path::new(path_str);
    //open the file in read/write mode
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;

    //get the length of the file
    let md = file.metadata()?;
    let size = md.len() as usize;

    //buffer of x's
    let x_vec = vec!['x' as u8; size];
    //buffer of 0's
    let zero_vec = vec![0; size];

    //write x's onto the file
    file.write_all(&x_vec)?;
    //write 0's onto the file
    file.write_all(&zero_vec)?;
    //delete the file
    fs::remove_file(path)?;

    Ok(())
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

    let fl = FileList::from_paths(items);

    let mut state = SxredderState {
        current_dir: ".".to_string(),
        file_list: fl.clone(),
        right_pane_content: Paragraph::new(""),
        confirm_delete: false,
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
        let (_li, pb) = state.selected_item();
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
            mnemonic_keys_text.push_str(" | [Enter] dir");
        } else {
            mnemonic_keys_text.push_str(" | s[X]red file");
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

        //show the warning popup if sxredding
        if state.confirm_delete {
            show_warning(&pb, frame);
        }
    };

    loop {
        terminal.draw(|f| draw_panels(f, &mut state))?;

        if state.confirm_delete {
            if let Event::Key(event) = read()? {
                match event.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        let (_li, pb) = state.file_list.selected_item();
                        state.confirm_delete = false;
                        sxred_file(pb.as_path()).expect("Could not sxred file!");
                        //TODO show success? at least in status line
                    },
                    KeyCode::Char('n') | KeyCode::Char('N') => {
                        state.confirm_delete = false;
                    },
                    _ => (),

                }
            }
        }else if let Event::Key(event) = read()? {
            match event.code {
                KeyCode::Char('q') => break,
                KeyCode::Up | KeyCode::Char('k') => state.move_up(),
                KeyCode::Down | KeyCode::Char('j') => state.move_down(),
                KeyCode::Left | KeyCode::Char('h') => {
                    let (_li, pb) = state.file_list.selected_item();
                    move_out_of_directory(&mut state, &pb)?;
                }
                KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                    let (_li, pb) = state.file_list.selected_item();
                    if pb.is_dir() {
                        enter_directory(&mut state, &pb)?;
                    }
                }
                KeyCode::Char('x') => {
                    let (_li, pb) = state.file_list.selected_item();
                    if pb.is_file() {
                        state.confirm_delete = true;
                    }
                }
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

///get confirmation before deleting
fn move_out_of_directory(state: &mut SxredderState, pb: &PathBuf) -> Result<(), Report> {
    //get the current directory
    if let Some(dir) = pb.parent() {
        //get the parent of the current directory
        if let Some(parent) = dir.parent() {
            println!(
                "This is the parent: {}",
                parent.as_os_str().to_str().unwrap()
            );
            let directories = read_directory(parent.as_os_str().to_str().unwrap_or("./"))?;
            let parent_list = FileList::from_paths(directories);
            state.file_list = parent_list;
        }
    }

    Ok(())
}

fn enter_directory(state: &mut SxredderState, pb: &PathBuf) -> Result<(), Report> {
    let new_items = read_directory(pb.to_str().unwrap())?;
    state.file_list = FileList::from_paths(new_items);
    state.current_dir = pb.to_str().unwrap().to_string();
    state.update_preview_pane_content(0);
    Ok(())
}

fn init_logging(opts: Opts) -> Subscriber {
    // Initialize the logging system based on the verbose flag
    FmtSubscriber::builder()
        .with_max_level(if opts.verbose {
            tracing::Level::INFO
        } else {
            tracing::Level::WARN
        })
        .finish()
}

fn read_current_dir() -> Result<Vec<PathBuf>, Report> {
    let curr_dir = std::env::current_dir()?;
    read_directory(curr_dir.as_os_str().to_str().unwrap())
}

pub fn read_directory(dir_path: &str) -> Result<Vec<PathBuf>, Report> {
    // println!("reading {dir_path}");
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
    use std::fs;
    use std::io::Write;
    use std::path::Path;

    #[test]
    fn test_shred_file() {
        let file_path = "test_file.txt";
        let file_content = "This is a test file content";

        // Create a file to test
        let mut file = fs::File::create(file_path).unwrap();
        file.write_all(file_content.as_bytes()).unwrap();

        let path = Path::new(file_path);
        // Shred the file
        let shred_result = sxred_file(path);
        assert!(shred_result.is_ok());

        // Check if file still exists
        assert!(!Path::new(file_path).exists());
    }

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

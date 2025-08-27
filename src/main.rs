use std::io;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use ratatui::{
    buffer::Buffer,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    symbols::border,
    layout::{Constraint, Layout, Rect},
    style::{
        Color, Style, Stylize,
    },
    text::Line,
    widgets::{
        Block, HighlightSpacing, List, ListItem, ListState, Paragraph,
        StatefulWidget, Widget,
    },
    DefaultTerminal,
    Frame
};

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let app_result = App::default().run(&mut terminal);
    ratatui::restore();
    app_result
}

#[derive(Debug)]
pub struct Config {
    pub base_wsl_path: PathBuf,
    pub base_windows_path: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_wsl_path: PathBuf::from("/home/"),
            base_windows_path: PathBuf::from("/mnt/c/Users/samy4/OneDrive/Desktop"),
        }
    }
}

#[derive(Debug)]
pub struct FileItem {
    name: String,
    is_directory: bool,
}

impl FileItem {
    fn new(name: String, is_directory: bool) -> Self {
        Self { name, is_directory }
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn is_directory(&self) -> bool {
        self.is_directory
    }
}

#[derive(Debug)]
pub struct FileList {
    items: Vec<FileItem>,
    state: ListState,
    current_path: PathBuf,
}

impl FileList {
    fn new(items: Vec<String>, path: PathBuf) -> Self {
        let mut state = ListState::default();
        state.select(Some(0)); // Select first item by default
        
        let file_items: Vec<FileItem> = items.into_iter().map(|name| {
            let is_dir = fs::metadata(&path.join(&name)).map(|m| m.is_dir()).unwrap_or(false);
            FileItem::new(name, is_dir)
        }).collect();
        
        Self {
            items: file_items,
            state,
            current_path: path,
        }
    }

    fn items(&self) -> &[FileItem] {
        &self.items
    }

    fn state_mut(&mut self) -> &mut ListState {
        &mut self.state
    }

    fn current_path(&self) -> &PathBuf {
        &self.current_path
    }

    fn select_next(&mut self) {
        self.state.select_next();
    }

    fn select_previous(&mut self) {
        self.state.select_previous();
    }

    fn navigate_into(&mut self) -> bool {
        if let Some(selected) = self.state.selected() {
            if let Some(item) = self.items.get(selected) {
                if item.name() == ".." {
                    return self.navigate_up();
                }
                let new_path = self.current_path.join(item.name());
                if new_path.is_dir() {
                    self.current_path = new_path;
                    self.refresh_items();
                    return true;
                }
            }
        }
        false
    }

    fn navigate_up(&mut self) -> bool {
        if let Some(parent) = self.current_path.parent() {
            self.current_path = parent.to_path_buf();
            self.refresh_items();
            return true;
        }
        false
    }

    fn refresh_items(&mut self) {
        let mut items = fs::read_dir(&self.current_path)
            .unwrap_or_else(|_| fs::read_dir("/").unwrap())
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let is_dir = entry.file_type().ok()?.is_dir();
                Some(FileItem::new(entry.file_name().to_string_lossy().to_string(), is_dir))
            })
            .collect::<Vec<FileItem>>();
        
        // Add ".." at the beginning for navigation
        items.insert(0, FileItem::new("..".to_string(), false));
        
        self.items = items;
        // Set the first item (..) as selected by default
        self.state.select(Some(0));
    }
}

#[derive(Debug)]
pub struct AppState {
    wsl_list: FileList,
    windows_list: FileList,
}

impl Default for AppState {
    fn default() -> Self {
        let config = Config::default();
        
        let mut wsl_items = fs::read_dir(&config.base_wsl_path)
            .unwrap_or_else(|_| fs::read_dir("/").unwrap())
            .filter_map(|entry| {
                entry.ok().map(|e| e.file_name().to_string_lossy().to_string())
            })
            .collect::<Vec<String>>();
        wsl_items.insert(0, "..".to_string());

        let mut windows_items = fs::read_dir(&config.base_windows_path)
            .unwrap_or_else(|_| fs::read_dir("/").unwrap())
            .filter_map(|entry| {
                entry.ok().map(|e| e.file_name().to_string_lossy().to_string())
            })
            .collect::<Vec<String>>();
        windows_items.insert(0, "..".to_string());

        Self {
            wsl_list: FileList::new(wsl_items, config.base_wsl_path),
            windows_list: FileList::new(windows_items, config.base_windows_path),
        }
    }
}

#[derive(Debug, Default)]
pub enum Focus {
    #[default]
    Wsl,
    Windows,
}

#[derive(Debug, Default)]
pub struct App {
    exit: bool,
    state: AppState,
    focus: Focus,
    status_message: Option<String>,
    status_timer: u8,
}

impl App {
    /// runs the application's main loop until the user quits
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        while !self.exit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events()?;
            self.clear_status();
        }
        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame) {
        frame.render_widget(self, frame.area());
    }

    /// updates the application's state based on user input
    fn handle_events(&mut self) -> io::Result<()> {
        match event::read()? {
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                self.handle_key_event(key_event)
            }
            _ => {}
        };
        Ok(())
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Char('q') => self.exit(),
            KeyCode::Tab => self.switch_focus(),
            KeyCode::Char('j') | KeyCode::Down => self.select_next(),
            KeyCode::Char('k') | KeyCode::Up => self.select_previous(),
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => self.navigate_into(),
            KeyCode::Char('h') | KeyCode::Left => self.navigate_up(),
            KeyCode::Char('e') => self.export_file(),
            KeyCode::Char('i') => self.import_file(),
            _ => {}
        }
    }

    fn switch_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Wsl => Focus::Windows,
            Focus::Windows => Focus::Wsl,
        };
    }

    fn select_next(&mut self) {
        match self.focus {
            Focus::Wsl => self.state.wsl_list.select_next(),
            Focus::Windows => self.state.windows_list.select_next(),
        }
    }

    fn select_previous(&mut self) {
        match self.focus {
            Focus::Wsl => self.state.wsl_list.select_previous(),
            Focus::Windows => self.state.windows_list.select_previous(),
        }
    }

    fn navigate_into(&mut self) {
        match self.focus {
            Focus::Wsl => { self.state.wsl_list.navigate_into(); }
            Focus::Windows => { self.state.windows_list.navigate_into(); }
        }
    }

    fn navigate_up(&mut self) {
        match self.focus {
            Focus::Wsl => { self.state.wsl_list.navigate_up(); }
            Focus::Windows => { self.state.windows_list.navigate_up(); }
        }
    }

    fn exit(&mut self) {
        self.exit = true;
    }

    fn export_file(&mut self) {
        // Export from WSL to Windows (always)
        if let Some(selected) = self.state.wsl_list.state.selected() {
            if let Some(item) = self.state.wsl_list.items().get(selected) {
                let source_path = if item.name() == ".." {
                    self.state.wsl_list.current_path.clone()
                } else {
                    self.state.wsl_list.current_path.join(item.name())
                };
                
                // Use the selected directory on Windows side as destination
                let dest_path = if let Some(windows_selected) = self.state.windows_list.state.selected() {
                    if let Some(windows_item) = self.state.windows_list.items().get(windows_selected) {
                        if windows_item.name() == ".." {
                            self.state.windows_list.current_path.join(item.name())
                        } else {
                            self.state.windows_list.current_path.join(windows_item.name()).join(item.name())
                        }
                    } else {
                        self.state.windows_list.current_path.join(item.name())
                    }
                } else {
                    self.state.windows_list.current_path.join(item.name())
                };
                
                self.copy_item(&source_path, &dest_path);
                self.show_status(&format!("Exported {} to Windows", item.name()));
            }
        }
    }

    fn import_file(&mut self) {
        // Import from Windows to WSL (always)
        if let Some(selected) = self.state.windows_list.state.selected() {
            if let Some(item) = self.state.windows_list.items().get(selected) {
                let source_path = if item.name() == ".." {
                    self.state.windows_list.current_path.clone()
                } else {
                    self.state.windows_list.current_path.join(item.name())
                };
                
                // Use the selected directory on WSL side as destination
                let dest_path = if let Some(wsl_selected) = self.state.wsl_list.state.selected() {
                    if let Some(wsl_item) = self.state.wsl_list.items().get(wsl_selected) {
                        if wsl_item.name() == ".." {
                            self.state.wsl_list.current_path.join(item.name())
                        } else {
                            self.state.wsl_list.current_path.join(wsl_item.name()).join(item.name())
                        }
                    } else {
                        self.state.wsl_list.current_path.join(item.name())
                    }
                } else {
                    self.state.wsl_list.current_path.join(item.name())
                };
                
                self.copy_item(&source_path, &dest_path);
                self.show_status(&format!("Imported {} to WSL", item.name()));
            }
        }
    }

    fn show_status(&mut self, message: &str) {
        self.status_message = Some(message.to_string());
        self.status_timer = 50;
    }

    fn clear_status(&mut self) {
        if self.status_timer > 0 {
            self.status_timer -= 1;
        } else {
            self.status_message = None;
        }
    }

    fn copy_item(&self, source: &PathBuf, dest: &PathBuf) {
        let source_str = source.to_string_lossy().to_string();
        let dest_str = dest.to_string_lossy().to_string();
        
        // Use cp -r for recursive copying (works for both files and directories)
        let output = Command::new("cp")
            .arg("-r")
            .arg(&source_str)
            .arg(&dest_str)
            .output();
            
        match output {
            Ok(result) => {
                if !result.status.success() {
                    let error = String::from_utf8_lossy(&result.stderr);
                    eprintln!("Failed to copy {} to {}: {}", source.display(), dest.display(), error);
                }
            }
            Err(e) => {
                eprintln!("Failed to execute cp command: {}", e);
            }
        }
    }
}

impl Widget for &mut App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // let title = Line::from(" File Browser ".bold());
        // let instructions = Line::from(vec![
        //     " Quit ".into(),
        //     "<Q>".blue().bold(),
        //     " Switch ".into(),
        //     "<Tab>".blue().bold(),
        //     " Navigate ".into(),
        //     "<J/K>".blue().bold(),
        //     " Enter ".into(),
        //     "<L/Enter>".blue().bold(),
        //     " Go Up ".into(),
        //     "<H/Left>".blue().bold(),
        //     " Copy ".into(),
        //     "<E/I>".blue().bold(),
        // ]);
        
        // let block = Block::bordered()
        //     .title(title.centered())
        //     .title_bottom(instructions.centered())
        //     .border_set(border::THICK);

        let [content_area, status_area] = Layout::vertical([
            // Constraint::Length(3),
            Constraint::Fill(1),
            Constraint::Length(1),
        ]).areas(area);

        // // Render header
        // Paragraph::new("")
        //     .block(block)
        //     .render(header_area, buf);

        // Render two columns
        let [wsl_area, windows_area] = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Fill(1),
        ]).areas(content_area);

        self.render_wsl_list(wsl_area, buf);
        self.render_windows_list(windows_area, buf);

        // Render status message
        if let Some(status) = &self.status_message {
            Paragraph::new(status.clone())
                .style(Style::new().fg(Color::Green))
                .centered()
                .render(status_area, buf);
        }
    }
}

impl App {
    fn render_wsl_list(&mut self, area: Rect, buf: &mut Buffer) {
        let is_focused = matches!(self.focus, Focus::Wsl);
        let current_path = self.state.wsl_list.current_path().display().to_string();
        let title = format!(" WSL: {} ", current_path).bold();
        
        let mut block = Block::bordered()
            .title(Line::from(title))
            .border_set(border::THICK);
        
        // Use green border for focused list
        if is_focused {
            block = block.border_style(Style::new().fg(Color::Green));
        }

        let items: Vec<ListItem> = self.state.wsl_list.items()
            .iter()
            .map(|item| {
                let style = if item.is_directory() {
                    Style::new().fg(Color::Blue)
                } else {
                    Style::new().fg(Color::White)
                };
                ListItem::new(item.name().to_string()).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(block)
            .highlight_symbol(">")
            .highlight_spacing(HighlightSpacing::Always);

        StatefulWidget::render(list, area, buf, self.state.wsl_list.state_mut());
    }

    fn render_windows_list(&mut self, area: Rect, buf: &mut Buffer) {
        let is_focused = matches!(self.focus, Focus::Windows);
        let current_path = self.state.windows_list.current_path().display().to_string();
        let title = format!(" Windows: {} ", current_path).bold();
        
        let mut block = Block::bordered()
            .title(Line::from(title))
            .border_set(border::THICK);
        
        // Use green border for focused list
        if is_focused {
            block = block.border_style(Style::new().fg(Color::Green));
        }

        let items: Vec<ListItem> = self.state.windows_list.items()
            .iter()
            .map(|item| {
                let style = if item.is_directory() {
                    Style::new().fg(Color::Blue)
                } else {
                    Style::new().fg(Color::White)
                };
                ListItem::new(item.name().to_string()).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(block)
            .highlight_symbol(">")
            .highlight_spacing(HighlightSpacing::Always);

        StatefulWidget::render(list, area, buf, self.state.windows_list.state_mut());
    }
}
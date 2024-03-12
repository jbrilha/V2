// use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
// use ratatui::{backend::CrosstermBackend, Terminal};
use core::panic;
use std::io::ErrorKind;
use crossterm::{cursor, event, execute, queue, style, terminal};
use crossterm::{event::*, terminal::ClearType};
use std::time::{Duration, Instant};
use std::{cmp, isize, usize};
use std::{
    env,
    fs,
    io::{self, stdout, Write},
    // vec,
    path::PathBuf,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const NAME: &str = env!("CARGO_PKG_NAME");
const TAB_STOP: usize = 4;
const MSG_TTL: u64 = 1;
const NO_FILE_NAME: &str = "[No Name]";
const HELP_MSG: &str = "Ctrl + Q to Quit";
const DIRTY: &str = "Unsaved changes!";

struct CleanUp;

impl Drop for CleanUp {
    fn drop(&mut self) {
        terminal::disable_raw_mode().expect("Unable to disable raw mode");
        Output::clear_screen().expect("Error");
    }
}

struct StatusMessage {
    message: Option<String>,
    set_time: Option<Instant>,
}

impl StatusMessage {
    fn new(initial_message: String) -> Self {
        Self {
            message: Some(initial_message),
            set_time: Some(Instant::now()),
        }
    }

    fn set_message(&mut self, message: String) {
        self.message = Some(message);
        self.set_time = Some(Instant::now())
    }

    fn message(&mut self) -> Option<&String> {
        self.set_time.and_then(|time| {
            if time.elapsed() > Duration::from_secs(MSG_TTL) {
                self.message = None;
                self.set_time = None;
                None
            } else {
                Some(self.message.as_ref().unwrap())
            }
        })
    }
}

struct Output {
    win_size: (usize, usize),
    editor_contents: EditorContents,
    editor_rows: EditorRows,
    cursor_controller: CursorController,
    status_message: StatusMessage,
    line_nr_padding: usize,
    dirty: u8,
}

impl Output {
    fn new() -> Self {
        let win_size = terminal::size()
            .map(|(x, y)| (x as usize, y as usize - 2))
            .unwrap();
        let mut out = Self {
            win_size,
            line_nr_padding: 0,
            editor_contents: EditorContents::new(),
            editor_rows: EditorRows::new(),
            cursor_controller: CursorController::new(win_size),
            status_message: StatusMessage::new(HELP_MSG.into()),
            dirty: 0,
        };

        out.line_nr_padding =
            out.editor_rows.nr_of_rows().checked_ilog10().unwrap_or(0) as usize + 2; // god I love rust

        out
    }

    fn clear_screen() -> io::Result<()> {
        execute!(stdout(), terminal::Clear(ClearType::All))?;
        execute!(stdout(), cursor::MoveTo(0, 0))
    }

    fn insert_char(&mut self, ch: char) {
        if self.cursor_controller.cursor_y == self.editor_rows.nr_of_rows() {
            self.editor_rows.insert_row();
            self.dirty = 1;
        }
        self.editor_rows
            .get_editor_row_mut(self.cursor_controller.cursor_y)
            .insert_char(self.cursor_controller.cursor_x, ch);
        self.cursor_controller.cursor_x += 1;
        self.dirty = 1;
        // self.cursor_controller.prev_cursor_x = self.cursor_controller.cursor_x;
    }

    fn draw_status_line(&mut self) {
        self.editor_contents
            .push_str(&style::Attribute::Reverse.to_string());

        let status = format!(
            "{}{} -- {} ",
            self.editor_rows
                .file_name
                .as_ref()
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                .unwrap_or(NO_FILE_NAME),
            if self.dirty > 0 {"*"} else {""},
            self.editor_rows.nr_of_rows()
        );
        let status_len = cmp::min(status.len(), self.win_size.0);

        let cursor_info = format!(
            "{}:{}",
            self.cursor_controller.cursor_y + 1,
            self.cursor_controller.cursor_x + 1
        );

        self.editor_contents.push_str(&status[..status_len]);
        for i in status_len..self.win_size.0 {
            if self.win_size.0 - i == cursor_info.len() {
                self.editor_contents.push_str(&cursor_info);
                break;
            }
            self.editor_contents.push(' ')
        }

        self.editor_contents
            .push_str(&style::Attribute::Reset.to_string());

        self.editor_contents.push_str("\r\n");
    }

    fn draw_status_message(&mut self) {
        queue!(
            self.editor_contents,
            terminal::Clear(ClearType::UntilNewLine)
        )
        .unwrap();

        if let Some(msg) = self.status_message.message() {
            self.editor_contents
                .push_str(&msg[..cmp::min(self.win_size.0, msg.len())]);
        }
    }

    fn draw_rows(&mut self) {
        let screen_rows = self.win_size.1;
        let screen_cols = self.win_size.0;

        for i in 0..screen_rows {
            let file_row = i + self.cursor_controller.row_offset;

            if file_row >= self.editor_rows.nr_of_rows() {
                if self.editor_rows.nr_of_rows() == 0 && i == screen_rows / 3 {
                    let mut welcome = format!("{}! --- v{}", NAME.to_uppercase(), VERSION);
                    if welcome.len() > screen_cols {
                        welcome.truncate(screen_cols)
                    }
                    let mut padding = (screen_cols - welcome.len()) / 2;
                    if padding != 0 {
                        self.editor_contents.push('~');
                        padding -= 1
                    }
                    (0..padding).for_each(|_| self.editor_contents.push(' '));
                    self.editor_contents.push_str(&welcome);
                } else {
                    self.editor_contents.push('~');
                    // self.editor_contents.push_str(&((i + 1).to_string() + "  "));
                }
            } else {
                let row = self.editor_rows.get_render(file_row);
                let col_offset = self.cursor_controller.col_offset;
                let row_offset = self.cursor_controller.row_offset;
                let line_nr = i + row_offset + 1;
                let cursor_y = self.cursor_controller.cursor_y;

                if cursor_y == line_nr - 1 {
                    let line_nr_str = line_nr.to_string();

                    let line_nr_formatted =
                        format!("{:<pad$} ", line_nr_str, pad = self.line_nr_padding);

                    self.editor_contents.push_str(&(line_nr_formatted));
                } else {
                    let rel_line_nr = (i + row_offset).abs_diff(cursor_y);

                    let rel_line_nr_str = rel_line_nr.to_string();
                    let rel_line_nr_formatted =
                        format!("{:>pad$} ", rel_line_nr_str, pad = self.line_nr_padding);

                    self.editor_contents.push_str(&(rel_line_nr_formatted)); // vim :set nornu basically
                }
                let len = if row.len() < col_offset {
                    0
                } else {
                    let len = row.len() - col_offset;
                    if len > screen_cols {
                        screen_cols
                    } else {
                        len
                    }
                };

                let start = if len == 0 { 0 } else { col_offset };

                self.editor_contents.push_str(&(row[start..start + len]));
            }

            queue!(
                self.editor_contents,
                terminal::Clear(ClearType::UntilNewLine)
            )
            .unwrap();

            // if i < screen_rows - 1 {
            self.editor_contents.push_str("\r\n");
            // }
        }
    }

    fn refresh_screen(&mut self) -> io::Result<()> {
        self.cursor_controller.scroll(&self.editor_rows);
        queue!(self.editor_contents, cursor::Hide, cursor::MoveTo(0, 0))?;
        self.draw_rows();
        self.draw_status_line();
        self.draw_status_message();
        let cursor_x = self.cursor_controller.render_x - self.cursor_controller.col_offset
            + self.line_nr_padding
            + 1;
        let cursor_y = self.cursor_controller.cursor_y - self.cursor_controller.row_offset;
        queue!(
            self.editor_contents,
            cursor::MoveTo(cursor_x as u16, cursor_y as u16),
            cursor::Show
        )?;
        self.editor_contents.flush()
    }

    fn move_cursor(&mut self, direction: KeyCode) {
        self.cursor_controller
            .move_cursor(direction, &self.editor_rows);
    }

    fn jump_cursor(&mut self, direction: KeyCode) {
        self.cursor_controller
            .jump_cursor(direction, &self.win_size, &self.editor_rows);
    }
}

struct CursorController {
    cursor_x: usize,
    cursor_y: usize,
    screen_cols: usize,
    screen_rows: usize,
    row_offset: usize,
    col_offset: usize,
    render_x: usize,

    prev_cursor_x: usize,
}

impl CursorController {
    fn new(win_size: (usize, usize)) -> CursorController {
        Self {
            cursor_x: 0,
            cursor_y: 0,
            screen_cols: win_size.0,
            screen_rows: win_size.1,
            row_offset: 0,
            col_offset: 0,
            render_x: 0,

            prev_cursor_x: 0,
        }
    }

    fn get_render_x(&self, row: &Row) -> usize {
        row.row_content[..self.cursor_x]
            .chars()
            .fold(0, |render_x, c| {
                if c == '\t' {
                    render_x + (TAB_STOP - 1) - (render_x % TAB_STOP) + 1
                } else {
                    render_x + 1
                }
            })
    }

    fn scroll(&mut self, editor_rows: &EditorRows) {
        self.render_x = 0;

        if self.cursor_y < editor_rows.nr_of_rows() {
            self.render_x = self.get_render_x(editor_rows.get_editor_row(self.cursor_y))
        }

        self.row_offset = cmp::min(self.row_offset, self.cursor_y);
        if self.cursor_y >= self.row_offset + self.screen_rows {
            self.row_offset = self.cursor_y - self.screen_rows + 1;
        }

        self.col_offset = cmp::min(self.col_offset, self.render_x);
        if self.render_x >= self.col_offset + self.screen_cols {
            self.col_offset = self.render_x - self.screen_cols + 1;
        }
    }

    fn jump_cursor(
        &mut self,
        direction: KeyCode,
        win_size: &(usize, usize),
        editor_rows: &EditorRows,
    ) {
        let screen_rows = win_size.1;
        let eof = editor_rows.nr_of_rows() - 1;
        let half_jump = screen_rows / 2;

        match direction {
            KeyCode::Char('L') => self.cursor_y = cmp::min(screen_rows + self.row_offset - 1, eof),
            KeyCode::Char('H') => self.cursor_y = self.row_offset,

            KeyCode::Char('d') => {
                self.cursor_y = cmp::min(self.cursor_y + half_jump, eof);
                self.row_offset = if eof <= self.row_offset + screen_rows {
                    self.row_offset
                } else {
                    cmp::min(self.row_offset + half_jump, eof - screen_rows + 1)
                }
            }
            KeyCode::Char('u') => {
                let ro_is = self.row_offset as isize;
                let cy_is = self.cursor_y as isize;
                let hj_is = half_jump as isize;

                self.cursor_y = cmp::max(cy_is - hj_is, 0) as usize;
                self.row_offset = cmp::max(ro_is - hj_is, 0) as usize;
            }

            KeyCode::Char('f') => {
                self.row_offset = cmp::min(self.row_offset + screen_rows - 1, eof);
                self.cursor_y = if self.cursor_y + screen_rows > eof {
                    eof
                } else {
                    self.row_offset - 1
                };
            }
            KeyCode::Char('b') => {
                let ro_is = self.row_offset as isize;

                self.cursor_y = if self.row_offset == 0 {
                    self.cursor_y
                } else if self.cursor_y < self.row_offset + screen_rows - 1 {
                    self.row_offset + 1
                } else {
                    self.cursor_y - screen_rows + 2
                };

                self.row_offset = cmp::max(ro_is - screen_rows as isize, 0) as usize;
                if ro_is == eof as isize {
                    self.cursor_y -= 2
                }
            }
            _ => unimplemented!(),
        }

        self.cursor_x = cmp::min(
            self.prev_cursor_x,
            editor_rows.get_render(self.cursor_y).len() - 1,
        );
    }

    fn move_cursor(&mut self, direction: KeyCode, editor_rows: &EditorRows) {
        let nr_of_rows = editor_rows.nr_of_rows();

        match direction {
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => {
                if self.cursor_x > 0 {
                    self.cursor_x -= 1;
                    self.prev_cursor_x = self.cursor_x;
                } else if direction == KeyCode::Backspace && self.cursor_y > 0 {
                    self.cursor_y -= 1;
                    self.cursor_x = editor_rows.get_render(self.cursor_y).len();
                    self.prev_cursor_x = self.cursor_x;
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.cursor_y < nr_of_rows - 1 {
                    self.cursor_y += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.cursor_y > 0 {
                    self.cursor_y -= 1;
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if self.cursor_y < nr_of_rows
                    && self.cursor_x < editor_rows.get_render(self.cursor_y).len()
                {
                    self.cursor_x += 1;

                    if self.prev_cursor_x < editor_rows.get_render(self.cursor_y).len() - 1 {
                        // without this condition the prev_cursor_x will update past the last char for
                        // some reason -- just placing - 1 in the previous condition crashes the program
                        // when trying to move right on empty lines
                        self.prev_cursor_x = self.cursor_x;
                    }
                }
            }
            KeyCode::Char('$') => {
                if self.cursor_y < nr_of_rows {
                    self.cursor_x = editor_rows.get_editor_row(self.cursor_y).row_content.len();
                    self.prev_cursor_x = self.cursor_x;
                }
            }
            KeyCode::Char('0') => {
                self.cursor_x = 0;
                self.prev_cursor_x = self.cursor_x;
            }
            KeyCode::Char('_') => {
                if self.cursor_y < nr_of_rows {
                    let row = &editor_rows.get_editor_row(self.cursor_y).row_content;
                    self.cursor_x = row.find(|c: char| !c.is_whitespace()).unwrap_or(0);
                    self.prev_cursor_x = self.cursor_x;
                }
            }
            _ => unimplemented!(),
        }

        // fn move_cursor_to_content()

        let row_len = if self.cursor_y < nr_of_rows {
            editor_rows.get_render(self.cursor_y).len()
        } else {
            0
        };

        self.cursor_x = if self.prev_cursor_x < row_len {
            self.prev_cursor_x
        } else {
            if row_len == 0 {
                0
            } else {
                row_len - 1
            }
        }
    }
}

struct Reader;

impl Reader {
    fn read_key(&self) -> io::Result<KeyEvent> {
        loop {
            if event::poll(Duration::from_millis(500))? {
                if let Event::Key(event) = event::read()? {
                    return Ok(event);
                }
            }
        }
    }
}

struct EditorContents {
    content: String,
}

impl EditorContents {
    fn new() -> Self {
        Self {
            content: String::new(),
        }
    }

    fn push(&mut self, ch: char) {
        self.content.push(ch)
    }

    fn push_str(&mut self, string: &str) {
        self.content.push_str(string)
    }
}

impl io::Write for EditorContents {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match std::str::from_utf8(buf) {
            Ok(s) => {
                self.content.push_str(s);
                Ok(s.len())
            }
            Err(_) => Err(io::ErrorKind::WriteZero.into()),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        let out = write!(stdout(), "{}", self.content);
        stdout().flush()?;
        self.content.clear();
        out
    }
}

#[derive(Default)]
struct Row {
    row_content: String,
    render: String,
}

impl Row {
    fn new(row_content: String, render: String) -> Self {
        Self {
            row_content,
            render,
        }
    }

    fn insert_char(&mut self, idx: usize, ch: char) {
        self.row_content.insert(idx, ch);
        EditorRows::render_row(self)
    }
}

struct EditorRows {
    row_contents: Vec<Row>,
    file_name: Option<PathBuf>,
}

impl EditorRows {
    fn new() -> Self {
        match env::args().nth(1) {
            None => Self {
                row_contents: Vec::new(),
                file_name: None,
            },
            Some(file) => Self::from_file(file.into()),
        }
    }

    fn save(&self) -> io::Result<usize> {
        match &self.file_name {
            None => Err(io::Error::new(ErrorKind::Other, "no file name!")),
            Some(name) => {
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .open(name)?;
                let contents: String = self
                    .row_contents
                    .iter()
                    .map(|it| it.row_content.as_str())
                    .collect::<Vec<&str>>()
                    .join("\n");
                file.set_len(contents.len() as u64)?;
                file.write_all(contents.as_bytes())?;
                Ok(contents.as_bytes().len())
            }
        }
    }

    fn from_file(file: PathBuf) -> Self {
        let file_contents = fs::read_to_string(&file).expect("Failed to read file");
        Self {
            file_name: Some(file),
            row_contents: file_contents
                .lines()
                .map(|it| {
                    let mut row = Row::new(it.into(), String::new());
                    Self::render_row(&mut row);
                    row
                })
                .collect(),
        }
    }

    fn get_render(&self, idx: usize) -> &String {
        &self.row_contents[idx].render
    }

    fn get_editor_row_mut(&mut self, idx: usize) -> &mut Row {
        &mut self.row_contents[idx]
    }

    fn get_editor_row(&self, idx: usize) -> &Row {
        &self.row_contents[idx]
    }

    fn nr_of_rows(&self) -> usize {
        self.row_contents.len()
    }

    fn insert_row(&mut self) {
        self.row_contents.push(Row::default());
    }

    // fn get_row(&self, idx:usize ) -> &str {
    //     &self.row_contents[idx]
    // }

    fn render_row(row: &mut Row) {
        let mut idx = 0;
        let cap = row
            .row_content
            .chars()
            .fold(0, |acc, next| acc + if next == 't' { TAB_STOP } else { 1 });

        row.render = String::with_capacity(cap);
        row.row_content.chars().for_each(|c| {
            idx += 1;
            if c == '\t' {
                row.render.push(' ');
                while idx % TAB_STOP != 0 {
                    row.render.push(' ');
                    idx += 1
                }
            } else {
                row.render.push(c);
            }
        })
    }
}

struct Editor {
    reader: Reader,
    output: Output,
    command: String,
}

impl Editor {
    fn new() -> Self {
        Self {
            reader: Reader,
            output: Output::new(),
            command: String::new(),
        }
    }

    fn save_file(&mut self) -> io::Result<bool> {
        match self.output.editor_rows.save() {
            Ok(len) => {
                self.output
                    .status_message
                    .set_message(format!("{}B written", len));
                self.output.dirty = 0;
                Ok(true)
            }
            Err(error) => {
                self.output
                    .status_message
                    .set_message(format!("Something went wrong :("));
                Err(error)
            }
        }
    }

    fn quit(&mut self) -> io::Result<bool> {
        if self.output.dirty > 0 {
            self.output.status_message.set_message(DIRTY.into());
            return Ok(true);
        }
        Ok(false)
    }

    fn parse_command(&mut self, command: String) -> io::Result<bool> {
        if command == "w" {
            return self.save_file()
            // return Ok(true)
        }
        if command == "q" {
            return self.quit();
        }
        if command == "wq" {
            match self.save_file() {
                Ok(..) => return Ok(false),
                Err(error) => return Err(error)
            }
        }
        Ok(true)
    }

    fn process_command(&mut self) -> io::Result<bool> {
        self.command.clear();
        loop {
            match self.reader.read_key()? {
                KeyEvent {
                    code: ch @ (KeyCode::Char(..) | KeyCode::Esc | KeyCode::Enter ),
                    modifiers: KeyModifiers::NONE,
                    kind: _,
                    state: _ 
                } => {
                        match ch { 
                            KeyCode::End => return Ok(true),
                            KeyCode::Enter => break,
                            KeyCode::Char(ch) => { 
                                self.command.push(ch)
                            }
                            _ => unimplemented!()
                        }
                    }
                _ => unimplemented!()
            }
        }

        self.parse_command(self.command.to_string())
    }

    fn process_keypress(&mut self) -> io::Result<bool> {
        match self.reader.read_key()? {
            KeyEvent {
                code: KeyCode::Char(':'),
                modifiers: KeyModifiers::NONE,
                kind: _,
                state: _,
            } => return self.process_command(),
            KeyEvent {
                code: KeyCode::Char('q'),
                modifiers: KeyModifiers::CONTROL,
                kind: _,
                state: _,
            } => return Ok(false),// { if self.output.dirty > 0 {
            //         self.output.status_message.set_message(DIRTY.into());
            //         return Ok(true);
            //     }
                 // return  Ok(false)
            //},
            KeyEvent {
                code: direction @ ( KeyCode::Char('H') | KeyCode::Char('L')  // high | low (jump w/o scroll)
                ),
                modifiers: KeyModifiers::SHIFT,
                kind: _,
                state: _,
            } => self.output.jump_cursor(direction),
            KeyEvent {
                code: direction @ ( KeyCode::Left   | KeyCode::Char('h') | KeyCode::Backspace |
                                    KeyCode::Down   | KeyCode::Char('j') |
                                    KeyCode::Up     | KeyCode::Char('k') |
                                    KeyCode::Right  | KeyCode::Char('l') |
                                    KeyCode::Char('$') | KeyCode::Char('0') |
                                    KeyCode::Char('_')
                ),
                modifiers: KeyModifiers::NONE,
                kind: _,
                state: _,
            } => self.output.move_cursor(direction),
            KeyEvent {
                code: direction @ ( KeyCode::Char('b') | KeyCode::Char('u') |   // vim PgUp | half PgUp
                                    KeyCode::Char('f') | KeyCode::Char('d')    // vim PgDn | half PgDn
                ),
                modifiers: KeyModifiers::CONTROL,
                kind: _,
                state: _,
            } => self.output.jump_cursor(direction),
            KeyEvent { code: code @ (KeyCode::Char(..) | KeyCode::Tab),
                modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                kind: _,
                state: _ 
            } => self.output.insert_char(match code {
                    KeyCode::Tab => '\t',
                    KeyCode::Char(ch) => ch,
                    _ => unreachable!(),
                }),
            _ => {}
        }
        Ok(true)
    }

    fn run(&mut self) -> io::Result<bool> {
        self.output.refresh_screen()?;
        self.process_keypress()
    }
}

// struct Command {
//     command: String,
// }
//
// impl Command {
//     fn new() -> Self {
//         Self {
//             command: "",
//         }
//     }
//     
// }

fn main() -> io::Result<()> {
    let _clean_up = CleanUp;

    terminal::enable_raw_mode()?;

    // let mut stdout = stdout();
    // execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    //
    // let backend = CrosstermBackend::new(stdout);
    // let mut terminal = Terminal::new(backend)?;
    //
    // terminal.clear()?;

    let mut editor = Editor::new();
    while editor.run()? {}

    Ok(())
}

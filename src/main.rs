use crossterm::{event::*, terminal::ClearType};
use crossterm::{cursor, event, execute, queue, terminal};
// use std::io::stdout;
// use std::io::{self, Write, stdout};
use std::time::Duration;
use std::{
    cmp,
    env,
    fs,
    vec,
    path::Path,
    io::{self, Write, stdout}
};

struct CleanUp;

impl Drop for CleanUp {
    fn drop(&mut self) {
        terminal::disable_raw_mode().expect("Unable to disable raw mode");
        Output::clear_screen().expect("Error");
    }
}

struct Output {
    win_size: (usize, usize),
    editor_contents: EditorContents,
    editor_rows: EditorRows,
    cursor_controller: CursorController,
}

impl Output {
    fn new() -> Self {
        let win_size = terminal::size()
            .map(|(x, y)| (x as usize, y as usize))
            .unwrap();
        Self {
            win_size,
            editor_contents: EditorContents::new(),
            editor_rows: EditorRows::new(),
            cursor_controller: CursorController::new(win_size),
        }
    }

    fn clear_screen() -> io::Result<()> {
        execute!(stdout(), terminal::Clear(ClearType::All))?;
        execute!(stdout(), cursor::MoveTo(0, 0))
    }

    fn draw_rows(&mut self) {
        let version = "0.0.1";
        let screen_rows = self.win_size.1;
        let screen_cols = self.win_size.0;

        for i in 0..screen_rows {
            if i >= self.editor_rows.nr_of_rows() {
                if self.editor_rows.nr_of_rows() == 0 && i == screen_rows / 3 {
                    let mut welcome = format!("V2! --- v{}", version);
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
                }

            } else {
                let len = cmp::min(self.editor_rows.get_row(i).len(), screen_cols);
                self.editor_contents
                    .push_str(&self.editor_rows.get_row(i)[..len])
            }

            queue!(
                self.editor_contents,
                terminal::Clear(ClearType::UntilNewLine)
            ).unwrap();

            if i < screen_rows - 1 {
                self.editor_contents.push_str("\r\n");
            }
        }
    }

    fn refresh_screen(&mut self) -> io::Result<()> {
        queue!(self.editor_contents, cursor::Hide, cursor::MoveTo(0, 0))?;
        self.draw_rows();
        let cursor_x = self.cursor_controller.cursor_x;
        let cursor_y = self.cursor_controller.cursor_y;
        queue!(
            self.editor_contents,
            cursor::MoveTo(cursor_x as u16, cursor_y as u16),
            cursor::Show
        )?;
        self.editor_contents.flush() /* add this line*/
    }

    fn move_cursor(&mut self, direction: KeyCode) {
        self.cursor_controller.move_cursor(direction);
    }
}

struct CursorController {
    cursor_x: usize,
    cursor_y: usize,
    screen_cols: usize,
    screen_rows: usize,
}

impl CursorController {
    fn new(win_size: (usize, usize)) -> CursorController {
        Self {
            cursor_x: 0,
            cursor_y: 0,
            screen_cols: win_size.0,
            screen_rows: win_size.1,
        }
    }

    fn move_cursor(&mut self, direction: KeyCode) {
        match direction {
            KeyCode::Char('h') | KeyCode::Left => {
                if self.cursor_x > 0 {
                    self.cursor_x -= 1;
                } 
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.cursor_y < self.screen_rows - 1{
                    self.cursor_y += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                // self.cursor_y = self.cursor_y.saturating_sub(1);
                if self.cursor_y > 0 {
                    self.cursor_y -= 1;
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if self.cursor_x < self.screen_cols - 1{
                    self.cursor_x += 1;
                }
            }
            _ => unimplemented!(),
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

struct EditorRows {
    row_contents: Vec<Box<str>>,
}

impl EditorRows {
    fn new() -> Self {
        let mut arg = env::args();

        match arg.nth(1) {
            None => Self {
                row_contents: Vec::new(),
            },
            Some(file) => Self::from_file(file.as_ref()),
        }

    }

    fn from_file(file: &Path) -> Self {
        let file_contents = fs::read_to_string(file).expect("Failed to read file");
        Self {
            row_contents: file_contents.lines().map(|it| it.into()).collect(),
        }
    }
    
    fn nr_of_rows(&self) -> usize {
        self.row_contents.len()
    }

    fn get_row(&self, idx:usize ) -> &str {
        &self.row_contents[idx]
    }
}

struct Editor {
    reader: Reader,
    output: Output,
}

impl Editor {
    fn new() -> Self {
        Self {
            reader: Reader,
            output: Output::new(),
        }
    }

    fn process_keypress(&mut self) -> io::Result<bool> {
        match self.reader.read_key()? {
            KeyEvent {
                code: KeyCode::Char('q'),
                modifiers: KeyModifiers::CONTROL,
                kind: _,
                state: _,
            } => return Ok(false),
            KeyEvent {
                code: direction @ ( KeyCode::Left   | KeyCode::Char('h') |
                                    KeyCode::Down   | KeyCode::Char('j') |
                                    KeyCode::Up     | KeyCode::Char('k') |
                                    KeyCode::Right  | KeyCode::Char('l')
                ),
                modifiers: KeyModifiers::NONE,
                kind: _,
                state: _,
            } => self.output.move_cursor(direction),
            _ => {}
        }
        Ok(true)
    }

    fn run(&mut self) -> io::Result<bool> {
        self.output.refresh_screen()?;
        self.process_keypress()
    }
}

fn main() -> io::Result<()> {
    let _clean_up = CleanUp;
    terminal::enable_raw_mode()?;
    /* modify */
    let mut editor = Editor::new();
    while editor.run()? {}
    /* end */
    Ok(())
}

// fn main() {
//     let _clean_up = CleanUp;
//     terminal::enable_raw_mode().expect("Could not turn on Raw mode");
//     /* add the following */
//     loop {
//         if let Event::Key(event) = event::read().expect("Failed to read line") {
//             match event {
//                 KeyEvent {
//                     code: KeyCode::Char('q'),
//                     modifiers: event::KeyModifiers::CONTROL,
//                     kind: _,
//                     state: _,
//                 } => break,
//                 _ => {
//                     //todo
//                 }
//             }
//             println!("{:?}\r", event.code);
//         };
//     }
// }

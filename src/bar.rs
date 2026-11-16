//! The toolbar on the first terminal row: back, forward, reload and the
//! address field, drawn as terminal cells.

use std::fmt::Write as _;
use std::io;

use katui::present::cells::render_cell;
use katui::proto::input::{InputEvent, Key, Mods};
use katui::scene::CellSize;
use katui::scene::cell::{self, Attrs, Buffer, Color, Position};

use crate::engine::Loading;

/// A one-line text field with a cursor and the readline keys.
// katui's TextField comes with katui-widgets, whose font stack fails
// cargo deny, so we keep a small field of our own.
#[derive(Debug, Default)]
struct Field {
    text: String,
    /// A byte offset on a character boundary.
    cursor: usize,
}

impl Field {
    fn set_text(&mut self, text: &str) {
        text.clone_into(&mut self.text);
        self.cursor = text.len();
    }

    fn split(&self) -> (&str, &str) {
        self.text.split_at(self.cursor)
    }

    fn insert(&mut self, text: &str) {
        self.text.insert_str(self.cursor, text);
        self.cursor += text.len();
    }

    fn remove(&mut self, from: usize, to: usize) {
        self.text.replace_range(from..to, "");
        self.cursor = from;
    }

    fn previous(&self) -> usize {
        self.split()
            .0
            .char_indices()
            .next_back()
            .map_or(0, |(at, _)| at)
    }

    fn next(&self) -> usize {
        self.split()
            .1
            .chars()
            .next()
            .map_or(self.cursor, |c| self.cursor + c.len_utf8())
    }

    /// Apply a key or paste; whether it was Enter.
    fn handle(&mut self, event: &InputEvent) -> bool {
        match event {
            InputEvent::Paste(text) => {
                let line: String = text
                    .chars()
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .collect();
                self.insert(&line);
            }
            InputEvent::Key(Key::Enter, _) => return true,
            InputEvent::Key(Key::Char(c), mods) if mods.ctrl => self.shortcut(*c),
            InputEvent::Key(Key::Char(c), mods) if !mods.alt => {
                self.insert(c.encode_utf8(&mut [0; 4]));
            }
            InputEvent::Key(key, _) => self.edit(*key),
            _ => {}
        }
        false
    }

    /// The Ctrl+letter editing keys of a shell prompt.
    fn shortcut(&mut self, letter: char) {
        let end = self.text.len();
        match letter {
            'a' => self.cursor = 0,
            'e' => self.cursor = end,
            'u' => self.remove(0, self.cursor),
            'k' => self.remove(self.cursor, end),
            _ => {}
        }
    }

    fn edit(&mut self, key: Key) {
        match key {
            Key::Home => self.cursor = 0,
            Key::End => self.cursor = self.text.len(),
            Key::Backspace => self.remove(self.previous(), self.cursor),
            Key::Delete => self.remove(self.cursor, self.next()),
            Key::Left => self.cursor = self.previous(),
            Key::Right => self.cursor = self.next(),
            _ => {}
        }
    }
}

/// The terminal rows the bar takes from the top of the screen.
pub(crate) const ROWS: u16 = 1;
/// Each button's width in columns, the glyph with a space either side.
const BUTTON: u16 = 3;
const SEARCH: &str = "https://duckduckgo.com/?q=";
const HINT: &str = "type an address or search";
const BAR: Color = Color::Indexed(236);
const FIELD: Color = Color::Indexed(238);
const TEXT: Color = Color::Indexed(252);
const FADED: Color = Color::Indexed(243);

/// What a click on the bar reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    Back,
    Forward,
    Reload,
    Address,
}

/// The target under `column` of the bar's row.
pub(crate) fn target(column: u16) -> Target {
    match column / BUTTON {
        0 => Target::Back,
        1 => Target::Forward,
        2 => Target::Reload,
        _ => Target::Address,
    }
}

/// What the address field asks for after a key.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// The field stays focused.
    Typing,
    /// Open this URL and hand focus back to the page.
    Open(String),
    /// Hand focus back to the page.
    Leave,
}

#[derive(Debug, Default)]
pub(crate) struct Bar {
    pub(crate) url: String,
    pub(crate) state: Loading,
    field: Field,
    focused: bool,
    /// Whether the field holds the whole address as a selection, which
    /// the first typed character replaces.
    selected: bool,
    shown: Option<Buffer>,
}

impl Bar {
    pub(crate) fn focused(&self) -> bool {
        self.focused
    }

    /// Focus the field holding `text`, all of it selected.
    pub(crate) fn focus(&mut self, text: &str) {
        self.field.set_text(text);
        self.focused = true;
        self.selected = !text.is_empty();
    }

    pub(crate) fn blur(&mut self) {
        self.focused = false;
        self.selected = false;
    }

    /// Apply a key or paste to the focused field.
    pub(crate) fn input(&mut self, event: &InputEvent) -> Outcome {
        if matches!(event, InputEvent::Key(Key::Escape, _)) {
            self.blur();
            return Outcome::Leave;
        }
        let typing = matches!(event, InputEvent::Paste(_))
            || matches!(event, InputEvent::Key(Key::Char(_), mods) if !mods.ctrl && !mods.alt);
        if self.selected && (typing || matches!(event, InputEvent::Key(Key::Backspace, _))) {
            self.field.set_text("");
            if !typing {
                self.selected = false;
                return Outcome::Typing;
            }
        }
        if matches!(event, InputEvent::Key(..) | InputEvent::Paste(_)) {
            self.selected = false;
        }
        if !self.field.handle(event) {
            return Outcome::Typing;
        }
        let Some(url) = destination(&self.field.text) else {
            return Outcome::Typing;
        };
        self.blur();
        Outcome::Open(url)
    }

    /// Draw the bar again in full on the next draw.
    pub(crate) fn invalidate(&mut self) {
        self.shown = None;
    }

    /// Write the cells of the bar that changed since the last draw.
    pub(crate) fn draw(&mut self, out: &mut Vec<u8>, columns: u16) -> io::Result<()> {
        let next = self.row(columns);
        let changed = match &self.shown {
            Some(shown) => cell::diff(shown, &next),
            None => cell::all_cells(&next),
        };
        for (at, cell) in &changed {
            render_cell(out, *at, cell)?;
        }
        if !changed.is_empty() {
            out.extend_from_slice(b"\x1b[0m");
        }
        self.shown = Some(next);
        Ok(())
    }

    /// The bar's one row of cells.
    fn row(&self, columns: u16) -> Buffer {
        let mut row = Buffer::blank(CellSize {
            columns,
            rows: ROWS,
        });
        let plain = |row: &mut Buffer, x: u16, text: &str, fg: Color| {
            row.set_string(Position { x, y: 0 }, text, fg, BAR, Attrs::EMPTY);
        };
        plain(&mut row, 0, &" ".repeat(usize::from(columns)), TEXT);
        let lit = |on: bool| if on { TEXT } else { FADED };
        plain(&mut row, 1, "←", lit(self.state.back));
        plain(&mut row, BUTTON + 1, "→", lit(self.state.forward));
        plain(
            &mut row,
            BUTTON * 2 + 1,
            if self.state.busy { "×" } else { "⟳" },
            TEXT,
        );
        let start = BUTTON * 3;
        let width = columns.saturating_sub(start + 1);
        self.field_cells(&mut row, start, width);
        row
    }

    /// The address field: the URL, or while focused the text with its
    /// cursor, scrolled to keep the cursor in view.
    fn field_cells(&self, row: &mut Buffer, start: u16, width: u16) {
        let blank = " ".repeat(usize::from(width));
        row.set_string(
            Position { x: start, y: 0 },
            &blank,
            TEXT,
            FIELD,
            Attrs::EMPTY,
        );
        let text_at = Position { x: start + 1, y: 0 };
        let room = usize::from(width.saturating_sub(2));
        if !self.focused {
            let shown: String = self.url.chars().take(room).collect();
            row.set_string(text_at, &shown, TEXT, FIELD, Attrs::EMPTY);
            return;
        }
        if self.field.text.is_empty() {
            let mut hint = HINT.chars().take(room);
            let first = hint.next().map(String::from).unwrap_or_default();
            row.set_string(text_at, &first, FADED, FIELD, Attrs::REVERSE);
            let rest: String = hint.collect();
            let after = Position {
                x: text_at.x + 1,
                y: 0,
            };
            row.set_string(after, &rest, FADED, FIELD, Attrs::EMPTY);
            return;
        }
        let (before, after) = self.field.split();
        let (shown, cursor) = window(before, after, room);
        let attrs = if self.selected {
            Attrs::REVERSE
        } else {
            Attrs::EMPTY
        };
        row.set_string(text_at, &shown, TEXT, FIELD, attrs);
        if !self.selected {
            let under = shown.chars().nth(cursor).unwrap_or(' ');
            let x = text_at.x + u16::try_from(cursor).unwrap_or(0);
            row.set_string(
                Position { x, y: 0 },
                &under.to_string(),
                TEXT,
                FIELD,
                Attrs::REVERSE,
            );
        }
    }
}

/// The part of `before` and `after` the cursor between them that fits in
/// `room` columns, and the cursor's column in it.
fn window(before: &str, after: &str, room: usize) -> (String, usize) {
    let behind = before.chars().count();
    let skip = (behind + 1).saturating_sub(room);
    let shown: String = before
        .chars()
        .chain(after.chars())
        .skip(skip)
        .take(room)
        .collect();
    (shown, behind - skip)
}

/// Where typed text goes: a search for words or a single word without a
/// dot, else the URL, host or file it names.
pub(crate) fn destination(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let scheme = text.contains("://") || text.starts_with("about:") || text.starts_with("data:");
    let addressy = text.contains(['.', ':', '/']) || text == "localhost";
    if !scheme && (text.contains(char::is_whitespace) || !addressy) {
        return Some(format!("{SEARCH}{}", query(text)));
    }
    Some(crate::url_from_argument(text))
}

/// `text` encoded as a URL query value.
fn query(text: &str) -> String {
    let mut encoded = String::new();
    for byte in text.bytes() {
        match byte {
            b' ' => encoded.push('+'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => {
                let _ = write!(encoded, "%{byte:02X}");
            }
        }
    }
    encoded
}

/// Whether `key` with `mods` is a bar shortcut a page never sees.
pub(crate) fn shortcut(key: Key, mods: Mods) -> Option<Shortcut> {
    match key {
        Key::Char('l') if mods.ctrl => Some(Shortcut::Focus),
        Key::Char('r') if mods.ctrl => Some(Shortcut::Reload),
        Key::Left if mods.alt => Some(Shortcut::Back),
        Key::Right if mods.alt => Some(Shortcut::Forward),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shortcut {
    Focus,
    Reload,
    Back,
    Forward,
}

#[cfg(test)]
mod tests {
    use katui::proto::input::{InputEvent, Key, Mods};

    use super::{Bar, Field, Outcome, Target, destination, target, window};

    #[test]
    fn words_search_and_addresses_open() {
        assert_eq!(
            destination("rust terminal  browser").as_deref(),
            Some("https://duckduckgo.com/?q=rust+terminal++browser")
        );
        assert_eq!(
            destination("katnav").as_deref(),
            Some("https://duckduckgo.com/?q=katnav")
        );
        assert_eq!(
            destination("c++ & café").as_deref(),
            Some("https://duckduckgo.com/?q=c%2B%2B+%26+caf%C3%A9")
        );
        assert_eq!(
            destination(" example.com ").as_deref(),
            Some("https://example.com")
        );
        assert_eq!(
            destination("localhost").as_deref(),
            Some("https://localhost")
        );
        assert_eq!(destination("about:blank").as_deref(), Some("about:blank"));
        assert_eq!(
            destination("data:text/html,<p>a b</p>").as_deref(),
            Some("data:text/html,<p>a b</p>")
        );
        assert_eq!(destination("   "), None);
    }

    #[test]
    fn columns_reach_buttons_then_the_field() {
        let targets: Vec<_> = (0..11).map(target).collect();
        assert_eq!(
            targets,
            [
                Target::Back,
                Target::Back,
                Target::Back,
                Target::Forward,
                Target::Forward,
                Target::Forward,
                Target::Reload,
                Target::Reload,
                Target::Reload,
                Target::Address,
                Target::Address,
            ]
        );
    }

    #[test]
    fn the_window_keeps_the_cursor_in_view() {
        assert_eq!(window("abc", "de", 10), ("abcde".to_owned(), 3));
        assert_eq!(window("abcdefgh", "", 4), ("fgh".to_owned(), 3));
        assert_eq!(window("abcdefgh", "ij", 4), ("fghi".to_owned(), 3));
    }

    fn key(bar: &mut Bar, key: Key) -> Outcome {
        bar.input(&InputEvent::Key(key, Mods::NONE))
    }

    #[test]
    fn typing_replaces_the_selected_address() {
        let mut bar = Bar::default();
        bar.focus("https://example.com/");
        for c in "rust".chars() {
            assert_eq!(key(&mut bar, Key::Char(c)), Outcome::Typing);
        }
        assert_eq!(
            key(&mut bar, Key::Enter),
            Outcome::Open("https://duckduckgo.com/?q=rust".to_owned())
        );
        assert!(!bar.focused());
    }

    #[test]
    fn the_field_edits_at_its_cursor() {
        let mut field = Field::default();
        field.set_text("héllo");
        let ctrl = Mods {
            ctrl: true,
            ..Mods::NONE
        };
        for event in [
            InputEvent::Key(Key::Left, Mods::NONE),
            InputEvent::Key(Key::Left, Mods::NONE),
            InputEvent::Key(Key::Backspace, Mods::NONE),
            InputEvent::Paste("a\nb".to_owned()),
            InputEvent::Key(Key::Delete, Mods::NONE),
        ] {
            field.handle(&event);
        }
        assert_eq!(field.split(), ("héa b", "o"));
        field.handle(&InputEvent::Key(Key::Char('u'), ctrl));
        assert_eq!(field.split(), ("", "o"));
        assert!(field.handle(&InputEvent::Key(Key::Enter, Mods::NONE)));
    }

    #[test]
    fn arrows_keep_the_address_and_escape_leaves() {
        let mut bar = Bar::default();
        bar.focus("example.com");
        key(&mut bar, Key::End);
        key(&mut bar, Key::Char('/'));
        assert_eq!(
            key(&mut bar, Key::Enter),
            Outcome::Open("https://example.com/".to_owned())
        );
        bar.focus("example.com");
        assert_eq!(key(&mut bar, Key::Escape), Outcome::Leave);
        assert!(!bar.focused());
    }
}

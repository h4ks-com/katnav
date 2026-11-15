//! Terminal keys as DOM keyboard events: the `key`, `code`, legacy key
//! code and text a US keyboard would produce, which is what games match on.

use katui::proto::input::{Key, Mods};
use serde_json::{Value, json};

/// The DOM side of one key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DomKey {
    pub(crate) key: String,
    pub(crate) code: String,
    pub(crate) key_code: u32,
    pub(crate) text: Option<String>,
}

const SHIFTED_DIGITS: &str = ")!@#$%^&*(";

/// Punctuation by its unshifted and shifted characters, `code` and legacy key code.
const PUNCTUATION: [(char, char, &str, u32); 11] = [
    ('-', '_', "Minus", 189),
    ('=', '+', "Equal", 187),
    ('[', '{', "BracketLeft", 219),
    (']', '}', "BracketRight", 221),
    ('\\', '|', "Backslash", 220),
    (';', ':', "Semicolon", 186),
    ('\'', '"', "Quote", 222),
    (',', '<', "Comma", 188),
    ('.', '>', "Period", 190),
    ('/', '?', "Slash", 191),
    ('`', '~', "Backquote", 192),
];

fn named(key: &str, code: &str, key_code: u32, text: Option<&str>) -> DomKey {
    DomKey {
        key: key.to_owned(),
        code: code.to_owned(),
        key_code,
        text: text.map(str::to_owned),
    }
}

fn character(c: char) -> DomKey {
    let typed = Some(c.to_string());
    let with = |code: String, key_code: u32| DomKey {
        key: c.to_string(),
        code,
        key_code,
        text: typed.clone(),
    };
    if c.is_ascii_alphabetic() {
        let upper = c.to_ascii_uppercase();
        return with(format!("Key{upper}"), u32::from(upper));
    }
    if c.is_ascii_digit() {
        return with(format!("Digit{c}"), u32::from(c));
    }
    if let Some(digit) = SHIFTED_DIGITS.find(c) {
        return with(format!("Digit{digit}"), 48 + digit as u32);
    }
    if c == ' ' {
        return with("Space".to_owned(), 32);
    }
    if let Some((_, _, code, key_code)) = PUNCTUATION
        .iter()
        .find(|(plain, shifted, _, _)| *plain == c || *shifted == c)
    {
        return with((*code).to_owned(), *key_code);
    }
    with(String::new(), 0)
}

/// The DOM key for `key`.
pub(crate) fn dom_key(key: Key) -> Option<DomKey> {
    Some(match key {
        Key::Char(c) => character(c),
        Key::Enter => named("Enter", "Enter", 13, Some("\r")),
        Key::Tab => named("Tab", "Tab", 9, None),
        Key::Backspace => named("Backspace", "Backspace", 8, None),
        Key::Delete => named("Delete", "Delete", 46, None),
        Key::Insert => named("Insert", "Insert", 45, None),
        Key::Escape => named("Escape", "Escape", 27, None),
        Key::Up => named("ArrowUp", "ArrowUp", 38, None),
        Key::Down => named("ArrowDown", "ArrowDown", 40, None),
        Key::Left => named("ArrowLeft", "ArrowLeft", 37, None),
        Key::Right => named("ArrowRight", "ArrowRight", 39, None),
        Key::Home => named("Home", "Home", 36, None),
        Key::End => named("End", "End", 35, None),
        Key::PageUp => named("PageUp", "PageUp", 33, None),
        Key::PageDown => named("PageDown", "PageDown", 34, None),
        Key::Function(n @ 1..=24) => {
            let name = format!("F{n}");
            return Some(DomKey {
                key: name.clone(),
                code: name,
                key_code: 111 + u32::from(n),
                text: None,
            });
        }
        Key::Function(_) => return None,
    })
}

/// The DevTools modifier mask: alt 1, ctrl 2, meta 4, shift 8.
pub(crate) fn modifiers(mods: Mods) -> u32 {
    u32::from(mods.alt) | u32::from(mods.ctrl) << 1 | u32::from(mods.shift) << 3
}

/// The `Input.dispatchKeyEvent` parameters for `key` going down or up.
/// A key with ctrl or alt held types no text, as in a desktop browser.
pub(crate) fn key_event(dom: &DomKey, mods: Mods, down: bool, repeat: bool) -> Value {
    let mut event = json!({
        "type": if down { "keyDown" } else { "keyUp" },
        "key": dom.key,
        "code": dom.code,
        "windowsVirtualKeyCode": dom.key_code,
        "nativeVirtualKeyCode": dom.key_code,
        "modifiers": modifiers(mods),
        "autoRepeat": repeat,
    });
    if let Some(text) = dom
        .text
        .as_ref()
        .filter(|_| down && !mods.ctrl && !mods.alt)
        && let Some(fields) = event.as_object_mut()
    {
        fields.insert("text".to_owned(), json!(text));
        fields.insert("unmodifiedText".to_owned(), json!(text));
    }
    event
}

#[cfg(test)]
mod tests {
    use katui::proto::input::{Key, Mods};

    use super::{dom_key, key_event, modifiers};

    #[test]
    fn letters_digits_and_shifted_symbols_name_their_physical_key() {
        let code = |c| dom_key(Key::Char(c)).map(|dom| (dom.code, dom.key_code));
        assert_eq!(code('z'), Some(("KeyZ".to_owned(), 90)));
        assert_eq!(code('Z'), Some(("KeyZ".to_owned(), 90)));
        assert_eq!(code('1'), Some(("Digit1".to_owned(), 49)));
        assert_eq!(code('!'), Some(("Digit1".to_owned(), 49)));
        assert_eq!(code(' '), Some(("Space".to_owned(), 32)));
        assert_eq!(code('?'), Some(("Slash".to_owned(), 191)));
    }

    #[test]
    fn named_keys_match_what_games_read() {
        let up = dom_key(Key::Up).expect("arrows have a dom key");
        assert_eq!(
            (up.key.as_str(), up.code.as_str(), up.key_code),
            ("ArrowUp", "ArrowUp", 38)
        );
        let f = dom_key(Key::Function(11)).expect("f11 has a dom key");
        assert_eq!((f.code.as_str(), f.key_code), ("F11", 122));
    }

    #[test]
    fn only_a_plain_press_types_text() {
        let a = dom_key(Key::Char('a')).expect("letters have a dom key");
        let ctrl = Mods {
            ctrl: true,
            ..Mods::NONE
        };
        assert_eq!(key_event(&a, Mods::NONE, true, false)["text"], "a");
        assert!(key_event(&a, ctrl, true, false).get("text").is_none());
        assert!(
            key_event(&a, Mods::NONE, false, false)
                .get("text")
                .is_none()
        );
        assert_eq!(key_event(&a, ctrl, true, false)["modifiers"], 2);
        assert_eq!(
            modifiers(Mods {
                shift: true,
                alt: true,
                ctrl: false
            }),
            9
        );
    }
}

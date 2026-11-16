//! Terminal keys as the physical keys of a US keyboard: the Windows key
//! code the DOM reports as `keyCode`, and the native key code each
//! platform derives `KeyboardEvent.code` from, which is what games match on.

use katui::proto::input::{Key, KeypadKey, ModifierKey, Mods};

/// One physical key: its DOM `code`, Windows key code, and native key
/// codes on macOS (virtual key codes) and Linux (X11 key codes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Physical {
    pub(crate) code: &'static str,
    pub(crate) windows: i32,
    mac: i32,
    x11: i32,
}

impl Physical {
    /// The native key code on the platform we run on.
    pub(crate) fn native(self) -> i32 {
        if cfg!(target_os = "macos") {
            self.mac
        } else {
            self.x11
        }
    }
}

const fn key(code: &'static str, windows: i32, mac: i32, x11: i32) -> Physical {
    Physical {
        code,
        windows,
        mac,
        x11,
    }
}

const LETTERS: [Physical; 26] = [
    key("KeyA", 65, 0x00, 38),
    key("KeyB", 66, 0x0B, 56),
    key("KeyC", 67, 0x08, 54),
    key("KeyD", 68, 0x02, 40),
    key("KeyE", 69, 0x0E, 26),
    key("KeyF", 70, 0x03, 41),
    key("KeyG", 71, 0x05, 42),
    key("KeyH", 72, 0x04, 43),
    key("KeyI", 73, 0x22, 31),
    key("KeyJ", 74, 0x26, 44),
    key("KeyK", 75, 0x28, 45),
    key("KeyL", 76, 0x25, 46),
    key("KeyM", 77, 0x2E, 58),
    key("KeyN", 78, 0x2D, 57),
    key("KeyO", 79, 0x1F, 32),
    key("KeyP", 80, 0x23, 33),
    key("KeyQ", 81, 0x0C, 24),
    key("KeyR", 82, 0x0F, 27),
    key("KeyS", 83, 0x01, 39),
    key("KeyT", 84, 0x11, 28),
    key("KeyU", 85, 0x20, 30),
    key("KeyV", 86, 0x09, 55),
    key("KeyW", 87, 0x0D, 25),
    key("KeyX", 88, 0x07, 53),
    key("KeyY", 89, 0x10, 29),
    key("KeyZ", 90, 0x06, 52),
];

const DIGITS: [Physical; 10] = [
    key("Digit0", 48, 0x1D, 19),
    key("Digit1", 49, 0x12, 10),
    key("Digit2", 50, 0x13, 11),
    key("Digit3", 51, 0x14, 12),
    key("Digit4", 52, 0x15, 13),
    key("Digit5", 53, 0x17, 14),
    key("Digit6", 54, 0x16, 15),
    key("Digit7", 55, 0x1A, 16),
    key("Digit8", 56, 0x1C, 17),
    key("Digit9", 57, 0x19, 18),
];

const SHIFTED_DIGITS: &str = ")!@#$%^&*(";

/// Punctuation by its unshifted and shifted characters.
const PUNCTUATION: [(char, char, Physical); 12] = [
    (' ', ' ', key("Space", 32, 0x31, 65)),
    ('-', '_', key("Minus", 189, 0x1B, 20)),
    ('=', '+', key("Equal", 187, 0x18, 21)),
    ('[', '{', key("BracketLeft", 219, 0x21, 34)),
    (']', '}', key("BracketRight", 221, 0x1E, 35)),
    ('\\', '|', key("Backslash", 220, 0x2A, 51)),
    (';', ':', key("Semicolon", 186, 0x29, 47)),
    ('\'', '"', key("Quote", 222, 0x27, 48)),
    (',', '<', key("Comma", 188, 0x2B, 59)),
    ('.', '>', key("Period", 190, 0x2F, 60)),
    ('/', '?', key("Slash", 191, 0x2C, 61)),
    ('`', '~', key("Backquote", 192, 0x32, 49)),
];

const FUNCTIONS: [Physical; 12] = [
    key("F1", 112, 0x7A, 67),
    key("F2", 113, 0x78, 68),
    key("F3", 114, 0x63, 69),
    key("F4", 115, 0x76, 70),
    key("F5", 116, 0x60, 71),
    key("F6", 117, 0x61, 72),
    key("F7", 118, 0x62, 73),
    key("F8", 119, 0x64, 74),
    key("F9", 120, 0x65, 75),
    key("F10", 121, 0x6D, 76),
    key("F11", 122, 0x67, 95),
    key("F12", 123, 0x6F, 96),
];

/// What one terminal key press means to the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Press {
    pub(crate) physical: Physical,
    /// The character the key types, when it types one.
    pub(crate) text: Option<char>,
    /// Whether shift is part of the key, as in `!` or `A`.
    pub(crate) shifted: bool,
}

fn character(c: char) -> Option<Press> {
    let press = |physical, shifted| Press {
        physical,
        text: Some(c),
        shifted,
    };
    if c.is_ascii_alphabetic() {
        let index = usize::from(c.to_ascii_uppercase() as u8 - b'A');
        return LETTERS
            .get(index)
            .map(|physical| press(*physical, c.is_ascii_uppercase()));
    }
    if let Some(digit) = c.to_digit(10) {
        return DIGITS
            .get(digit as usize)
            .map(|physical| press(*physical, false));
    }
    if let Some(digit) = SHIFTED_DIGITS.find(c) {
        return DIGITS.get(digit).map(|physical| press(*physical, true));
    }
    PUNCTUATION
        .iter()
        .find(|(plain, shifted, _)| *plain == c || *shifted == c)
        .map(|(plain, _, physical)| press(*physical, *plain != c))
}

const MODIFIERS: [(ModifierKey, Physical); 8] = [
    (ModifierKey::LeftShift, key("ShiftLeft", 16, 0x38, 50)),
    (ModifierKey::RightShift, key("ShiftRight", 16, 0x3C, 62)),
    (ModifierKey::LeftControl, key("ControlLeft", 17, 0x3B, 37)),
    (
        ModifierKey::RightControl,
        key("ControlRight", 17, 0x3E, 105),
    ),
    (ModifierKey::LeftAlt, key("AltLeft", 18, 0x3A, 64)),
    (ModifierKey::RightAlt, key("AltRight", 18, 0x3D, 108)),
    (ModifierKey::LeftSuper, key("MetaLeft", 91, 0x37, 133)),
    (ModifierKey::RightSuper, key("MetaRight", 92, 0x36, 134)),
];

const KEYPAD_DIGITS: [Physical; 10] = [
    key("Numpad0", 96, 0x52, 90),
    key("Numpad1", 97, 0x53, 87),
    key("Numpad2", 98, 0x54, 88),
    key("Numpad3", 99, 0x55, 89),
    key("Numpad4", 100, 0x56, 83),
    key("Numpad5", 101, 0x57, 84),
    key("Numpad6", 102, 0x58, 85),
    key("Numpad7", 103, 0x59, 79),
    key("Numpad8", 104, 0x5B, 80),
    key("Numpad9", 105, 0x5C, 81),
];

const KEYPAD: [(KeypadKey, Physical, char); 5] = [
    (KeypadKey::Add, key("NumpadAdd", 107, 0x45, 86), '+'),
    (
        KeypadKey::Subtract,
        key("NumpadSubtract", 109, 0x4E, 82),
        '-',
    ),
    (
        KeypadKey::Multiply,
        key("NumpadMultiply", 106, 0x43, 63),
        '*',
    ),
    (KeypadKey::Divide, key("NumpadDivide", 111, 0x4B, 106), '/'),
    (KeypadKey::Decimal, key("NumpadDecimal", 110, 0x41, 91), '.'),
];

/// A keypad key, which types its digit or symbol.
fn keypad(key: KeypadKey) -> Option<Press> {
    let (physical, text) = match key {
        KeypadKey::Digit(digit) => (
            *KEYPAD_DIGITS.get(usize::from(digit))?,
            char::from_digit(u32::from(digit), 10)?,
        ),
        other => KEYPAD
            .iter()
            .find(|(named, _, _)| *named == other)
            .map(|(_, physical, text)| (*physical, *text))?,
    };
    Some(Press {
        physical,
        text: Some(text),
        shifted: false,
    })
}

const NAMED: [(Key, Physical); 15] = [
    (Key::CapsLock, key("CapsLock", 20, 0x39, 66)),
    (Key::Enter, key("Enter", 13, 0x24, 36)),
    (Key::Tab, key("Tab", 9, 0x30, 23)),
    (Key::Backspace, key("Backspace", 8, 0x33, 22)),
    (Key::Delete, key("Delete", 46, 0x75, 119)),
    (Key::Insert, key("Insert", 45, 0x72, 118)),
    (Key::Escape, key("Escape", 27, 0x35, 9)),
    (Key::Up, key("ArrowUp", 38, 0x7E, 111)),
    (Key::Down, key("ArrowDown", 40, 0x7D, 116)),
    (Key::Left, key("ArrowLeft", 37, 0x7B, 113)),
    (Key::Right, key("ArrowRight", 39, 0x7C, 114)),
    (Key::Home, key("Home", 36, 0x73, 110)),
    (Key::End, key("End", 35, 0x77, 115)),
    (Key::PageUp, key("PageUp", 33, 0x74, 112)),
    (Key::PageDown, key("PageDown", 34, 0x79, 117)),
];

fn named(key: Key) -> Option<Physical> {
    if let Key::Function(number) = key {
        return FUNCTIONS.get(usize::from(number).checked_sub(1)?).copied();
    }
    if let Key::Modifier(modifier) = key {
        return MODIFIERS
            .iter()
            .find(|(named, _)| *named == modifier)
            .map(|(_, physical)| *physical);
    }
    NAMED
        .iter()
        .find(|(named, _)| *named == key)
        .map(|(_, physical)| *physical)
}

/// The physical key and text behind `key`, or `None` for a key a US
/// keyboard does not have.
pub(crate) fn press(key: Key) -> Option<Press> {
    match key {
        Key::Char(c) => character(c),
        Key::Keypad(pad) => keypad(pad),
        Key::Enter => named(key).map(|physical| Press {
            physical,
            text: Some('\r'),
            shifted: false,
        }),
        other => named(other).map(|physical| Press {
            physical,
            text: None,
            shifted: false,
        }),
    }
}

/// CEF's modifier flags for `mods`, with shift added when the key needs it.
pub(crate) fn flags(mods: Mods, shifted: bool) -> u32 {
    const SHIFT: u32 = 1 << 1;
    const CONTROL: u32 = 1 << 2;
    const ALT: u32 = 1 << 3;
    const COMMAND: u32 = 1 << 7;
    let bit = |on: bool, flag: u32| if on { flag } else { 0 };
    bit(mods.shift || shifted, SHIFT)
        | bit(mods.ctrl, CONTROL)
        | bit(mods.alt, ALT)
        | bit(mods.super_key, COMMAND)
}

#[cfg(test)]
mod tests {
    use katui::proto::input::{Key, KeypadKey, ModifierKey, Mods};

    use super::{flags, press};

    fn code(key: Key) -> Option<(&'static str, i32, bool)> {
        press(key).map(|press| (press.physical.code, press.physical.windows, press.shifted))
    }

    #[test]
    fn characters_name_their_physical_key() {
        assert_eq!(code(Key::Char('z')), Some(("KeyZ", 90, false)));
        assert_eq!(code(Key::Char('Z')), Some(("KeyZ", 90, true)));
        assert_eq!(code(Key::Char('1')), Some(("Digit1", 49, false)));
        assert_eq!(code(Key::Char('!')), Some(("Digit1", 49, true)));
        assert_eq!(code(Key::Char(' ')), Some(("Space", 32, false)));
        assert_eq!(code(Key::Char('?')), Some(("Slash", 191, true)));
        assert_eq!(code(Key::Char('é')), None);
    }

    #[test]
    fn named_keys_match_what_games_read() {
        assert_eq!(code(Key::Up), Some(("ArrowUp", 38, false)));
        assert_eq!(code(Key::Function(11)), Some(("F11", 122, false)));
        assert_eq!(
            code(Key::Modifier(ModifierKey::LeftShift)),
            Some(("ShiftLeft", 16, false))
        );
        let five = press(Key::Keypad(KeypadKey::Digit(5))).expect("keypad 5 is a key");
        assert_eq!((five.physical.code, five.text), ("Numpad5", Some('5')));
        assert_eq!(code(Key::Function(30)), None);
        let enter = press(Key::Enter).expect("enter is a key");
        assert_eq!(enter.text, Some('\r'));
    }

    #[test]
    fn every_physical_key_has_distinct_native_codes() {
        let all: Vec<_> = ('a'..='z')
            .chain('0'..='9')
            .chain(" -=[]\\;',./`".chars())
            .filter_map(|c| press(Key::Char(c)))
            .map(|press| press.physical)
            .collect();
        for (index, physical) in all.iter().enumerate() {
            for other in all.iter().skip(index + 1) {
                assert_ne!(
                    physical.mac, other.mac,
                    "{} and {}",
                    physical.code, other.code
                );
                assert_ne!(
                    physical.x11, other.x11,
                    "{} and {}",
                    physical.code, other.code
                );
            }
        }
    }

    #[test]
    fn shift_comes_from_the_key_or_the_modifiers() {
        assert_eq!(flags(Mods::NONE, true), 2);
        let ctrl_alt = Mods {
            ctrl: true,
            alt: true,
            ..Mods::NONE
        };
        assert_eq!(flags(ctrl_alt, false), 12);
        let command = Mods {
            super_key: true,
            ..Mods::NONE
        };
        assert_eq!(flags(command, false), 128, "super is the command key");
    }
}

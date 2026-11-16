//! Game controllers for the page. Chromium only reads controllers for the
//! app in front, which is the terminal, so katui reads them for us and we
//! give the page a `navigator.getGamepads` that returns them.

use std::fmt::Write as _;

use katui::rt::gamepad::Pad;

/// What installs our `getGamepads` once, swaps in `pads` and fires the
/// connect and disconnect events a game listens for.
const SCRIPT: &str = "(function (pads) {
  if (!window.__katnavPads) {
    Object.defineProperty(Navigator.prototype, 'getGamepads', {
      value: function () { return window.__katnavPads || []; },
      configurable: true,
    });
  }
  const before = window.__katnavPads || [];
  const now = [];
  for (const pad of pads) {
    now[pad.index] = {
      id: pad.id, index: pad.index, connected: true, mapping: 'standard',
      timestamp: performance.now(), axes: pad.axes, vibrationActuator: null,
      buttons: pad.buttons.map((value) => ({ pressed: value > 0.5, touched: value > 0, value })),
    };
  }
  window.__katnavPads = now;
  const fire = (name, gamepad) => dispatchEvent(Object.assign(new Event(name), { gamepad }));
  now.forEach((pad) => { if (pad && !before[pad.index]) fire('gamepadconnected', pad); });
  before.forEach((pad) => { if (pad && !now[pad.index]) fire('gamepaddisconnected', pad); });
})";

/// The script that hands `pads` to the page.
pub(crate) fn script(pads: &[Pad]) -> String {
    let mut out = String::from(SCRIPT);
    out.push_str("([");
    for pad in pads {
        let _ = write!(
            out,
            "{{index:{},id:{},axes:{},buttons:{}}},",
            pad.index,
            quoted(&pad.name),
            numbers(&pad.axes),
            numbers(&pad.buttons),
        );
    }
    out.push_str("]);");
    out
}

fn numbers(values: &[f32]) -> String {
    let joined: Vec<String> = values.iter().map(|value| format!("{value:.4}")).collect();
    format!("[{}]", joined.join(","))
}

/// `text` as a JavaScript string literal.
fn quoted(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            c if c.is_control() || c == '\u{2028}' || c == '\u{2029}' => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use katui::rt::gamepad::Pad;

    use super::{quoted, script};

    #[test]
    fn a_pad_becomes_a_literal_in_the_standard_layout() {
        let mut pad = Pad {
            index: 1,
            name: "PS5 Controller".to_owned(),
            buttons: [0.0; 17],
            axes: [0.0; 4],
        };
        pad.buttons[0] = 1.0;
        pad.axes[0] = -0.5;
        let text = script(&[pad]);
        assert!(text.contains(
            "{index:1,id:\"PS5 Controller\",axes:[-0.5000,0.0000,0.0000,0.0000],buttons:[1.0000,"
        ));
        assert!(text.ends_with("]);"));
    }

    #[test]
    fn a_name_cannot_break_out_of_its_string() {
        assert_eq!(quoted("a\"b\\c\n"), "\"a\\\"b\\\\c\\u000a\"");
    }
}

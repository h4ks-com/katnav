//! One page on the terminal: its frames out through katui, the
//! terminal's keys, mouse and paste in through the DevTools input domain.

#![allow(
    clippy::indexing_slicing,
    reason = "we only index serde_json objects we built, which never panics"
)]

use std::io;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use katui::present::session;
use katui::present::surface::{PatchedKitty, PixelScreen};
use katui::proto::input::{Button, InputEvent, Key, MouseAction, MouseEvent, Pointer};
use katui::proto::probe::{Capabilities, CellPixels};
use katui::rt::app::{App, AppEvent, Control};
use katui::rt::events::Waker;
use katui::scene::CellSize;
use katui::scene::surface::Surface;
use serde_json::json;

use crate::chrome::Browser;
use crate::keys::{dom_key, key_event, modifiers};

const BASE_IMAGE: u32 = 900;
const TILE_BUDGET: usize = 320;
/// How far one wheel notch scrolls, in CSS pixels.
const WHEEL_STEP: f64 = 80.0;
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
/// How many times fewer pixels each way we ask for under a multiplexer,
/// which carries graphics far more slowly than a terminal does.
const MULTIPLEXED_SHRINK: u32 = 2;

/// What the protocol thread sends the loop.
#[derive(Debug)]
pub(crate) enum Message {
    Frame(Arc<Surface>),
    Navigated(String),
    Gone(String),
}

/// The page's viewport in CSS pixels and how many device pixels each one takes.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Viewport {
    width: u32,
    height: u32,
    scale: f64,
}

/// The CSS viewport for a terminal of `size` cells of `cell` pixels.
/// High density cells get two device pixels per CSS pixel, so pages lay
/// out at the size a desktop window of the same physical size would.
fn viewport(size: CellSize, cell: CellPixels, multiplexed: bool) -> Viewport {
    let device_width = u32::from(size.columns) * u32::from(cell.width);
    let device_height = u32::from(size.rows) * u32::from(cell.height);
    let density = if cell.height >= 30 { 2 } else { 1 };
    let shrink = if multiplexed { MULTIPLEXED_SHRINK } else { 1 };
    Viewport {
        width: device_width / density,
        height: device_height / density,
        scale: f64::from(density) / f64::from(shrink),
    }
}

pub(crate) struct Tab {
    browser: Browser,
    waker: Arc<OnceLock<Waker<Message>>>,
    caps: Capabilities,
    cell: CellPixels,
    url: String,
    screen: PixelScreen,
    fitted: Option<CellSize>,
    view: Viewport,
    frame: Option<Arc<Surface>>,
    pointer: Pointer,
    last_press: Option<(Instant, Button, u32)>,
    held_buttons: u32,
    releases_seen: bool,
    held_keys: Vec<Key>,
    started: bool,
    title_shown: bool,
    ended: Option<String>,
}

impl Tab {
    /// A tab on `browser` that opens `url`; `waker` is where the protocol
    /// thread finds the loop once it runs.
    pub(crate) fn new(
        browser: Browser,
        caps: Capabilities,
        url: String,
        waker: Arc<OnceLock<Waker<Message>>>,
    ) -> Self {
        let cell = katui::rt::tty::cell_size(&caps);
        Self {
            browser,
            waker,
            pointer: Pointer::new(caps.cell, cell),
            caps,
            cell,
            url,
            screen: PixelScreen::default(),
            fitted: None,
            view: Viewport {
                width: 0,
                height: 0,
                scale: 1.0,
            },
            frame: None,
            last_press: None,
            held_buttons: 0,
            releases_seen: false,
            held_keys: Vec::new(),
            started: false,
            title_shown: false,
            ended: None,
        }
    }

    /// Why the browser went away, once it has.
    pub(crate) fn ended(&self) -> Option<&str> {
        self.ended.as_deref()
    }

    /// Size the page to the terminal and restart the screencast at the
    /// new size, since the screencast caps its frames at what we asked for.
    fn fit(&mut self, out: &mut Vec<u8>, size: CellSize) -> io::Result<()> {
        if self.fitted == Some(size) {
            return Ok(());
        }
        self.cell = katui::rt::tty::cell_size(&self.caps);
        self.pointer.resize(size.columns, size.rows);
        self.view = viewport(size, self.cell, self.caps.multiplexed());
        let frame_width = (f64::from(self.view.width) * self.view.scale).round();
        let frame_height = (f64::from(self.view.height) * self.view.scale).round();
        self.browser.send(
            "Emulation.setDeviceMetricsOverride",
            json!({
                "width": self.view.width,
                "height": self.view.height,
                "deviceScaleFactor": self.view.scale,
                "mobile": false,
            }),
        );
        self.browser.send("Page.stopScreencast", json!({}));
        self.browser.send(
            "Page.startScreencast",
            json!({
                "format": "jpeg",
                "quality": 85,
                "maxWidth": frame_width,
                "maxHeight": frame_height,
            }),
        );
        self.screen.free(out)?;
        self.screen = PixelScreen::new(
            &self.caps,
            self.cell,
            PatchedKitty::with_tiles(
                BASE_IMAGE,
                self.cell,
                u32::from(self.cell.height) * 3,
                TILE_BUDGET,
            ),
        );
        self.fitted = Some(size);
        Ok(())
    }

    fn open(&mut self) {
        self.started = true;
        self.browser.send("Page.enable", json!({}));
        self.browser
            .send("Page.navigate", json!({ "url": self.url }));
    }

    /// The CSS pixel under `mouse`.
    fn css_point(&mut self, mouse: &MouseEvent) -> (f64, f64) {
        let (x, y) = self.pointer.locate(mouse, self.cell);
        let fitted = self.fitted.unwrap_or(CellSize {
            columns: 1,
            rows: 1,
        });
        let across = f64::from(fitted.columns) * f64::from(self.cell.width);
        let down = f64::from(fitted.rows) * f64::from(self.cell.height);
        (
            f64::from(x) * f64::from(self.view.width) / across.max(1.0),
            f64::from(y) * f64::from(self.view.height) / down.max(1.0),
        )
    }

    fn click_count(&mut self, button: Button) -> u32 {
        let now = Instant::now();
        let count = match self.last_press {
            Some((at, last, count)) if last == button && now - at < DOUBLE_CLICK => count + 1,
            _ => 1,
        };
        self.last_press = Some((now, button, count));
        count
    }

    fn mouse(&mut self, mouse: &MouseEvent) {
        let (x, y) = self.css_point(mouse);
        let mut event = json!({
            "x": x,
            "y": y,
            "modifiers": modifiers(mouse.mods),
        });
        match mouse.action {
            MouseAction::Press(button) => {
                self.held_buttons |= button_bit(button);
                event["type"] = json!("mousePressed");
                event["button"] = json!(button.name());
                event["clickCount"] = json!(self.click_count(button));
            }
            MouseAction::Release(button) => {
                self.held_buttons &= !button_bit(button);
                event["type"] = json!("mouseReleased");
                event["button"] = json!(button.name());
                event["clickCount"] = json!(self.last_press.map_or(1, |(_, _, count)| count));
            }
            MouseAction::Drag(button) => {
                event["type"] = json!("mouseMoved");
                event["button"] = json!(button.name());
            }
            MouseAction::Move => event["type"] = json!("mouseMoved"),
            MouseAction::WheelUp | MouseAction::WheelDown => {
                let sign = if mouse.action == MouseAction::WheelUp {
                    -1.0
                } else {
                    1.0
                };
                event["type"] = json!("mouseWheel");
                event["deltaX"] = json!(0);
                event["deltaY"] = json!(sign * WHEEL_STEP);
            }
        }
        event["buttons"] = json!(self.held_buttons);
        self.browser.send("Input.dispatchMouseEvent", event);
    }

    /// A key going down. Until the terminal has shown it reports releases
    /// we release every key right after its press, so no key sticks.
    fn key_down(&mut self, key: Key, mods: katui::proto::input::Mods) {
        let Some(dom) = dom_key(key) else { return };
        let repeat = self.held_keys.contains(&key);
        self.browser.send(
            "Input.dispatchKeyEvent",
            key_event(&dom, mods, true, repeat),
        );
        if self.releases_seen {
            if !repeat {
                self.held_keys.push(key);
            }
        } else {
            self.browser.send(
                "Input.dispatchKeyEvent",
                key_event(&dom, mods, false, false),
            );
        }
    }

    fn key_up(&mut self, key: Key, mods: katui::proto::input::Mods) {
        self.releases_seen = true;
        self.held_keys.retain(|held| *held != key);
        if let Some(dom) = dom_key(key) {
            self.browser.send(
                "Input.dispatchKeyEvent",
                key_event(&dom, mods, false, false),
            );
        }
    }

    fn input(&mut self, event: &InputEvent) -> Control {
        match event {
            InputEvent::Key(Key::Char('q'), mods) if mods.ctrl => return Control::Quit,
            InputEvent::Key(key, mods) => self.key_down(*key, *mods),
            InputEvent::KeyUp(key, mods) => self.key_up(*key, *mods),
            InputEvent::Mouse(mouse) => self.mouse(mouse),
            InputEvent::Paste(text) => self
                .browser
                .send("Input.insertText", json!({ "text": text })),
            InputEvent::Mode(..) => self.pointer.observe(event),
            InputEvent::Focus(false) => {
                for key in std::mem::take(&mut self.held_keys) {
                    self.key_up(key, katui::proto::input::Mods::NONE);
                }
            }
            _ => {}
        }
        Control::Wait
    }
}

fn button_bit(button: Button) -> u32 {
    match button {
        Button::Left => 1,
        Button::Right => 2,
        Button::Middle => 4,
    }
}

impl App for Tab {
    type Message = Message;

    fn start(&mut self, waker: Waker<Message>) {
        let _ = self.waker.set(waker);
    }

    fn update(&mut self, event: AppEvent<Message>) -> Control {
        match event {
            AppEvent::Input(input) => self.input(&input),
            AppEvent::Message(Message::Frame(frame)) => {
                self.frame = Some(frame);
                Control::Redraw
            }
            AppEvent::Message(Message::Navigated(url)) => {
                self.url = url;
                self.title_shown = false;
                Control::Redraw
            }
            AppEvent::Message(Message::Gone(reason)) => {
                self.ended = Some(reason);
                Control::Quit
            }
            AppEvent::Resized(_) => Control::Redraw,
            AppEvent::Repaint => {
                self.screen.repaint();
                Control::Redraw
            }
        }
    }

    fn draw(&mut self, out: &mut Vec<u8>, size: CellSize) -> io::Result<Control> {
        if !self.started {
            session::report_key_releases(out)?;
        }
        self.fit(out, size)?;
        if !self.started {
            self.open();
        }
        if !self.title_shown {
            self.title_shown = true;
            write_title(out, &self.url)?;
        }
        if let Some(frame) = &self.frame {
            self.screen
                .render_frame(out, frame, size.columns, size.rows)?;
        }
        Ok(Control::Wait)
    }
}

/// The page URL as the terminal window title.
fn write_title(out: &mut Vec<u8>, url: &str) -> io::Result<()> {
    use std::io::Write as _;
    let clean: String = url.chars().filter(|c| !c.is_control()).collect();
    write!(out, "\x1b]2;katnav {clean}\x1b\\")
}

#[cfg(test)]
mod tests {
    use katui::proto::probe::CellPixels;
    use katui::scene::CellSize;

    use super::viewport;

    #[test]
    fn dense_cells_lay_out_at_half_the_device_pixels() {
        let size = CellSize {
            columns: 100,
            rows: 40,
        };
        let plain = viewport(
            size,
            CellPixels {
                width: 10,
                height: 20,
            },
            false,
        );
        assert_eq!((plain.width, plain.height, plain.scale), (1000, 800, 1.0));
        let retina = viewport(
            size,
            CellPixels {
                width: 20,
                height: 40,
            },
            false,
        );
        assert_eq!(
            (retina.width, retina.height, retina.scale),
            (1000, 800, 2.0)
        );
        let multiplexed = viewport(
            size,
            CellPixels {
                width: 20,
                height: 40,
            },
            true,
        );
        assert_eq!(multiplexed.scale, 1.0, "half the pixels each way travel");
    }
}

//! One page on the terminal: CEF's frames out through katui, the
//! terminal's keys, mouse and paste in through CEF's input calls.

use std::cell::RefCell;
use std::io;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use katui::present::session;
use katui::present::surface::{PatchedKitty, PixelScreen};
use katui::proto::input::{Button, InputEvent, Key, Mods, MouseAction, MouseEvent, Pointer};
use katui::proto::probe::{Capabilities, CellPixels};
use katui::rt::app::{App, AppEvent, Control, Delivery};
use katui::rt::events::Waker;
use katui::rt::gamepad::{Pad, watch};
use katui::rt::stash::local_stash;
use katui::scene::CellSize;
use katui::scene::surface::{PixelPos, Surface};

use crate::Error;
use crate::engine::{self, Engine, KeyKind, Page, View};
use crate::governor::Governor;
use crate::keys;
use crate::pads;
use crate::trace::{self, Trace};

const BASE_IMAGE: u32 = 900;
const TILE_BUDGET: usize = 320;
pub(crate) const FRAMES_PER_SECOND: u32 = 60;
/// How often we run CEF's message loop.
const PUMP_EVERY: Duration = Duration::from_millis(2);
/// How far one wheel notch scrolls.
const WHEEL_STEP: i32 = 100;
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
/// How long we wait for the page to close before we leave anyway.
const CLOSE_WAIT: Duration = Duration::from_secs(1);
const IS_REPEAT: u32 = 1 << 13;
/// A CEF pump past this long gets a trace line.
const SLOW_PUMP: Duration = Duration::from_millis(8);

/// What the pump thread sends the loop.
#[derive(Debug)]
pub(crate) enum Message {
    /// Time to run CEF's message loop.
    Pump,
    /// The game controllers changed; their newest state waits in the tab.
    Pads,
}

/// The render scale range the governor picks from: pages start sharp, and
/// a game shrinks until its frames keep up.
fn scale_range(multiplexed: bool) -> (f64, f64) {
    let (min, max) = (0.25, 1.0);
    if multiplexed {
        (min / 2.0, max / 2.0)
    } else {
        (min, max)
    }
}

/// The most pixels we let CEF paint a frame at. Above about this, copying
/// every frame back from the GPU falls behind and input waits behind the
/// queued frames; the terminal scales the picture up to fill its cells.
const MOST_PAINTED_PIXELS: f64 = 2_000_000.0;

/// The largest scale that keeps a `width` by `height` CSS pixel view under
/// [`MOST_PAINTED_PIXELS`].
fn paint_cap(width: i32, height: i32) -> f64 {
    let area = f64::from(width.max(1)) * f64::from(height.max(1));
    (MOST_PAINTED_PIXELS / area).sqrt()
}

/// CSS pixels for a terminal of `size` cells of `cell` device pixels:
/// cells 30 pixels tall or more are a high density screen.
fn layout(size: CellSize, cell: CellPixels) -> (i32, i32, f64) {
    let density = if cell.height >= 30 { 2 } else { 1 };
    let width = u32::from(size.columns) * u32::from(cell.width) / density;
    let height = u32::from(size.rows) * u32::from(cell.height) / density;
    let as_int = |value: u32| i32::try_from(value).unwrap_or(i32::MAX);
    (as_int(width), as_int(height), f64::from(density))
}

pub(crate) struct Tab {
    page: Option<Page>,
    engine: Engine,
    view: Rc<RefCell<View>>,
    url: String,
    caps: Capabilities,
    cell: CellPixels,
    density: f64,
    screen: PixelScreen,
    fitted: Option<CellSize>,
    frame: Option<Surface>,
    /// When CEF handed us the frame we hold, and when the one we drew last.
    frame_at: Option<Instant>,
    drawn_frame_at: Option<Instant>,
    trace: Trace,
    pointer: Pointer,
    last_press: Option<(Instant, Button, i32)>,
    held_buttons: u32,
    releases_seen: bool,
    held_keys: Vec<Key>,
    governor: Governor,
    pump_pending: Arc<AtomicBool>,
    title_shown: bool,
    closing: Option<Instant>,
    pads: Vec<Pad>,
    latest_pads: Arc<Mutex<Option<Vec<Pad>>>>,
}

impl Tab {
    pub(crate) fn new(engine: Engine, caps: Capabilities, url: String) -> Self {
        let cell = katui::rt::tty::cell_size(&caps);
        let (min, max) = scale_range(caps.multiplexed());
        Self {
            page: None,
            engine,
            view: Rc::new(RefCell::new(View::default())),
            url,
            pointer: Pointer::new(caps.cell, cell),
            caps,
            cell,
            density: 1.0,
            screen: PixelScreen::default(),
            fitted: None,
            frame: None,
            frame_at: None,
            drawn_frame_at: None,
            trace: Trace::from_env(),
            last_press: None,
            held_buttons: 0,
            releases_seen: false,
            held_keys: Vec::new(),
            governor: Governor::new(Duration::from_secs(1) / FRAMES_PER_SECOND, min, max),
            pump_pending: Arc::new(AtomicBool::new(false)),
            title_shown: false,
            closing: None,
            pads: Vec::new(),
            latest_pads: Arc::default(),
        }
    }

    /// Whether the page closed, which CEF needs before it can shut down.
    pub(crate) fn closed(&self) -> bool {
        self.view.borrow().closed
    }

    fn set_scale(&self) {
        let mut view = self.view.borrow_mut();
        let wanted = self.density * self.governor.scale();
        view.scale = wanted.min(paint_cap(view.width, view.height)) as f32;
    }

    /// Size the page to the terminal, opening it the first time.
    fn fit(&mut self, out: &mut Vec<u8>, size: CellSize) -> Result<(), Error> {
        if self.fitted == Some(size) {
            return Ok(());
        }
        self.cell = katui::rt::tty::cell_size(&self.caps);
        self.pointer.resize(size.columns, size.rows);
        let (width, height, density) = layout(size, self.cell);
        self.density = density;
        {
            let mut view = self.view.borrow_mut();
            view.width = width;
            view.height = height;
        }
        self.set_scale();
        if self.page.is_none() {
            self.page = Some(self.engine.open(&self.url, &self.view)?);
        }
        if let Some(page) = &self.page {
            page.resized();
        }
        self.screen.free(out)?;
        let mut presenter = PatchedKitty::with_tiles(
            BASE_IMAGE,
            self.cell,
            u32::from(self.cell.height) * 3,
            TILE_BUDGET,
        );
        if let Some(stash) = local_stash(&self.caps) {
            presenter = presenter.with_stash(stash);
        }
        self.screen = PixelScreen::new(&self.caps, self.cell, presenter);
        self.fitted = Some(size);
        Ok(())
    }

    /// Hand the page the game controllers as they are now.
    fn show_pads(&self) {
        if let Some(page) = &self.page {
            page.run_script(&pads::script(&self.pads));
        }
    }

    /// Run CEF and collect what it reported.
    fn pump(&mut self) -> Control {
        let started = Instant::now();
        self.engine.pump();
        if self.trace.on() && started.elapsed() > SLOW_PUMP {
            let took = trace::millis(started.elapsed());
            self.trace.line(format_args!("pump {took:.1} ms"));
        }
        // we queue the next pump only once this one is done, so pumps never
        // pile up while a frame is on its way
        self.pump_pending.store(false, Ordering::Relaxed);
        let mut view = self.view.borrow_mut();
        if view.closed || self.closing.is_some_and(|at| at.elapsed() > CLOSE_WAIT) {
            return Control::Quit;
        }
        let mut control = Control::Wait;
        if let Some(address) = view.address.take() {
            self.url = address;
            self.title_shown = false;
            self.show_pads();
            control = Control::Redraw;
        }
        if let Some(frame) = view.frame.take() {
            if self.trace.on() {
                let middle = PixelPos::new(frame.width() / 2, frame.height() / 2);
                let color = frame.get(middle).map(|pixel| (pixel.r, pixel.g, pixel.b));
                let (width, height) = (frame.width(), frame.height());
                self.trace
                    .line(format_args!("paint {width}x{height} middle {color:?}"));
            }
            self.frame = Some(frame);
            self.frame_at = Some(Instant::now());
            control = Control::Redraw;
        }
        control
    }

    /// The CSS pixel under `mouse`.
    fn css_point(&mut self, mouse: &MouseEvent) -> (i32, i32) {
        let (x, y) = self.pointer.locate(mouse, self.cell);
        let ratio = 1.0 / self.density;
        ((f64::from(x) * ratio) as i32, (f64::from(y) * ratio) as i32)
    }

    fn click_count(&mut self, button: Button) -> i32 {
        let now = Instant::now();
        let count = match self.last_press {
            Some((at, last, count)) if last == button && now - at < DOUBLE_CLICK => count + 1,
            _ => 1,
        };
        self.last_press = Some((now, button, count));
        count
    }

    fn mouse(&mut self, mouse: &MouseEvent) {
        let at = self.css_point(mouse);
        if let MouseAction::Press(button) = mouse.action {
            self.held_buttons |= button_flag(button);
        }
        let count = match mouse.action {
            MouseAction::Press(button) => self.click_count(button),
            _ => self.last_press.map_or(1, |(_, _, count)| count),
        };
        let flags = keys::flags(mouse.mods, false) | self.held_buttons;
        if let MouseAction::Release(button) = mouse.action {
            self.held_buttons &= !button_flag(button);
        }
        let Some(page) = &self.page else { return };
        match mouse.action {
            MouseAction::Press(button) => page.click(at, flags, button, false, count),
            MouseAction::Release(button) => page.click(at, flags, button, true, count),
            MouseAction::Drag(_) | MouseAction::Move => page.hover(at, flags),
            MouseAction::WheelUp => page.wheel(at, flags, WHEEL_STEP),
            MouseAction::WheelDown => page.wheel(at, flags, -WHEEL_STEP),
        }
    }

    /// A key going down. Until the terminal has shown it reports releases
    /// we release every key right after its press, so no key sticks.
    fn key_down(&mut self, key: Key, mods: Mods) {
        let (Some(page), Some(press)) = (&self.page, keys::press(key)) else {
            return;
        };
        let repeat = self.held_keys.contains(&key);
        let flags = keys::flags(mods, press.shifted) | if repeat { IS_REPEAT } else { 0 };
        let character = press.text.map_or(0, char_code);
        let event = |kind, windows| engine::Key {
            kind,
            windows,
            native: press.physical.native(),
            character,
            flags,
        };
        page.key(&event(KeyKind::Down, press.physical.windows));
        if let Some(text) = press.text.filter(|_| !mods.ctrl && !mods.alt) {
            page.key(&event(KeyKind::Char, i32::from(char_code(text))));
        }
        if !self.releases_seen {
            page.key(&event(KeyKind::Up, press.physical.windows));
        } else if !repeat {
            self.held_keys.push(key);
        }
    }

    fn key_up(&mut self, key: Key, mods: Mods) {
        self.releases_seen = true;
        self.held_keys.retain(|held| *held != key);
        let (Some(page), Some(press)) = (&self.page, keys::press(key)) else {
            return;
        };
        page.key(&engine::Key {
            kind: KeyKind::Up,
            windows: press.physical.windows,
            native: press.physical.native(),
            character: press.text.map_or(0, char_code),
            flags: keys::flags(mods, press.shifted),
        });
    }

    fn input(&mut self, event: &InputEvent) -> Control {
        match event {
            InputEvent::Key(Key::Char('q' | 'c'), mods) if mods.ctrl => {
                if let Some(page) = &self.page {
                    page.close();
                }
                self.closing = Some(Instant::now());
            }
            InputEvent::Key(key, mods) => {
                self.trace.line(format_args!("key {key:?}"));
                self.key_down(*key, *mods);
            }
            InputEvent::KeyUp(key, mods) => self.key_up(*key, *mods),
            InputEvent::Mouse(mouse) => self.mouse(mouse),
            InputEvent::Paste(text) => {
                if let Some(page) = &self.page {
                    page.insert(text);
                }
            }
            InputEvent::Mode(..) => self.pointer.observe(event),
            InputEvent::Focus(false) => {
                for key in std::mem::take(&mut self.held_keys) {
                    self.key_up(key, Mods::NONE);
                }
            }
            _ => {}
        }
        Control::Wait
    }
}

/// The UTF-16 unit CEF takes for `c`; characters outside the basic
/// plane type nothing.
fn char_code(c: char) -> u16 {
    u16::try_from(u32::from(c)).unwrap_or(0)
}

fn button_flag(button: Button) -> u32 {
    match button {
        Button::Left => 1 << 4,
        Button::Middle => 1 << 5,
        Button::Right => 1 << 6,
    }
}

impl App for Tab {
    type Message = Message;

    fn start(&mut self, waker: Waker<Message>) {
        // a controller reports far faster than we draw, so we keep only its
        // newest state and wake the loop once for it, never once per report
        let latest = Arc::clone(&self.latest_pads);
        let pads = waker.clone();
        // without controller support the page plays from the keyboard
        let _ = watch(move |now| {
            let Ok(mut slot) = latest.lock() else {
                return false;
            };
            let waiting = slot.replace(now.to_vec()).is_some();
            drop(slot);
            waiting || pads.send(Message::Pads)
        });
        let pending = Arc::clone(&self.pump_pending);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(PUMP_EVERY);
                if !pending.swap(true, Ordering::Relaxed) && !waker.send(Message::Pump) {
                    return;
                }
            }
        });
    }

    fn update(&mut self, event: AppEvent<Message>) -> Control {
        match event {
            AppEvent::Message(Message::Pump) => self.pump(),
            AppEvent::Message(Message::Pads) => {
                if let Some(pads) = self
                    .latest_pads
                    .lock()
                    .ok()
                    .and_then(|mut slot| slot.take())
                {
                    self.trace.line(format_args!("pads {}", pads.len()));
                    self.pads = pads;
                    self.show_pads();
                }
                Control::Wait
            }
            AppEvent::Input(input) => self.input(&input),
            AppEvent::Resized(_) => Control::Redraw,
            AppEvent::Repaint => {
                self.screen.repaint();
                Control::Redraw
            }
        }
    }

    fn draw(&mut self, out: &mut Vec<u8>, size: CellSize) -> io::Result<Control> {
        if self.page.is_none() {
            session::report_key_releases(out)?;
        }
        self.fit(out, size).map_err(io::Error::other)?;
        if !self.title_shown {
            self.title_shown = true;
            write_title(out, &self.url)?;
        }
        if let Some(frame) = &self.frame {
            let written = self
                .screen
                .render_frame(out, frame, size.columns, size.rows)?;
            if written > 0 {
                self.drawn_frame_at = self.frame_at;
            }
        }
        Ok(Control::Wait)
    }

    /// Let the governor judge how long the frame took to encode and reach
    /// the terminal.
    fn delivered(&mut self, delivery: &Delivery) {
        if self.trace.on() {
            let age = self
                .drawn_frame_at
                .map_or(0.0, |at| trace::millis(at.elapsed()));
            let (drawn, sent) = (
                trace::millis(delivery.drawn),
                trace::millis(delivery.delivered),
            );
            let (bytes, answered) = (delivery.bytes, delivery.answered);
            self.trace.line(format_args!(
                "frame {bytes} B, encoded {drawn:.1} ms, delivered {sent:.1} ms, answered {answered}, frame age {age:.1} ms"
            ));
        }
        if self
            .governor
            .delivered(delivery.delivered, Instant::now())
            .is_some()
        {
            self.set_scale();
            if let Some(page) = &self.page {
                page.resized();
            }
        }
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

    use super::{layout, scale_range};

    #[test]
    fn dense_cells_lay_out_at_half_the_device_pixels() {
        let size = CellSize {
            columns: 100,
            rows: 40,
        };
        let plain = CellPixels {
            width: 10,
            height: 20,
        };
        let retina = CellPixels {
            width: 20,
            height: 40,
        };
        assert_eq!(layout(size, plain), (1000, 800, 1.0));
        assert_eq!(layout(size, retina), (1000, 800, 2.0));
    }

    #[test]
    fn a_multiplexer_halves_the_scale_range() {
        assert_eq!(scale_range(false), (0.25, 1.0));
        assert_eq!(scale_range(true), (0.125, 0.5));
    }
}

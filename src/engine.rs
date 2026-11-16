//! Chromium through CEF, rendering off screen: CEF calls us with each
//! painted frame, and we hand it keys and mouse events. Everything here
//! runs on the main thread, which pumps CEF's message loop.

#![allow(
    clippy::transmute_ptr_to_ptr,
    reason = "cef's wrap macros expand to these transmutes"
)]

use std::cell::RefCell;
use std::fs::File;
use std::os::raw::c_int;
use std::path::PathBuf;
use std::rc::Rc as Shared;

use cef::args::Args;
use cef::{
    App, Browser, BrowserSettings, CefString, CefStringUtf16, Client, CommandLine, CursorInfo,
    CursorType, DictionaryValue, DisplayHandler, Frame, ImplApp, ImplBrowser, ImplBrowserHost,
    ImplClient, ImplCommandLine, ImplDisplayHandler, ImplFrame, ImplLifeSpanHandler,
    ImplRenderHandler, KeyEvent, KeyEventType, LifeSpanHandler, MouseButtonType, MouseEvent,
    PaintElementType, PopupFeatures, Range, Rect, RenderHandler, ScreenInfo, Settings, WindowInfo,
    WindowOpenDisposition, WrapApp, WrapClient, WrapDisplayHandler, WrapLifeSpanHandler,
    WrapRenderHandler, rc::Rc as _, wrap_app, wrap_client, wrap_display_handler,
    wrap_life_span_handler, wrap_render_handler,
};
use katui::present::session::PointerShape;
use katui::scene::surface::{PixelRect, Surface};

use crate::Error;

/// What the page and katnav share: the view size CEF asks for, and what
/// CEF reported since we last looked.
#[derive(Debug, Default)]
pub(crate) struct View {
    /// The viewport in CSS pixels.
    pub(crate) width: i32,
    pub(crate) height: i32,
    /// Device pixels per CSS pixel.
    pub(crate) scale: f32,
    /// The newest frame, until the tab takes it.
    pub(crate) frame: Option<Surface>,
    /// Where the frames painted since the tab last took one changed.
    pub(crate) damage: Vec<PixelRect>,
    /// The pointer shape the page asked for, until the tab shows it.
    pub(crate) pointer: Option<PointerShape>,
    /// The page's new address, until the tab takes it.
    pub(crate) address: Option<String>,
    pub(crate) closed: bool,
}

type ViewCell = Shared<RefCell<View>>;

#[derive(Clone)]
struct Handlers {
    view: ViewCell,
}

/// A dirty rectangle from CEF in frame pixels.
fn pixel_rect(rect: &Rect) -> Option<PixelRect> {
    Some(PixelRect {
        x: u32::try_from(rect.x).ok()?,
        y: u32::try_from(rect.y).ok()?,
        width: u32::try_from(rect.width).ok()?,
        height: u32::try_from(rect.height).ok()?,
    })
}

/// The terminal pointer shape closest to a CEF cursor.
fn pointer_shape(cursor: CursorType) -> PointerShape {
    match cursor {
        CursorType::HAND => PointerShape::Pointer,
        CursorType::IBEAM | CursorType::VERTICALTEXT => PointerShape::Text,
        CursorType::CROSS | CursorType::CELL => PointerShape::Crosshair,
        CursorType::MOVE => PointerShape::Move,
        CursorType::WAIT => PointerShape::Wait,
        CursorType::PROGRESS => PointerShape::Progress,
        CursorType::HELP => PointerShape::Help,
        CursorType::NOTALLOWED | CursorType::NODROP => PointerShape::NotAllowed,
        CursorType::GRAB => PointerShape::Grab,
        CursorType::GRABBING => PointerShape::Grabbing,
        CursorType::EASTWESTRESIZE
        | CursorType::EASTRESIZE
        | CursorType::WESTRESIZE
        | CursorType::COLUMNRESIZE => PointerShape::ColResize,
        CursorType::NORTHSOUTHRESIZE
        | CursorType::NORTHRESIZE
        | CursorType::SOUTHRESIZE
        | CursorType::ROWRESIZE => PointerShape::RowResize,
        CursorType::NONE => PointerShape::Hidden,
        _ => PointerShape::Default,
    }
}

/// A BGRA frame from CEF as an RGBA surface.
fn surface_from_bgra(bgra: &[u8], width: u32, height: u32) -> Option<Surface> {
    let mut rgba = bgra.to_vec();
    let (pixels, _) = rgba.as_chunks_mut::<4>();
    for pixel in pixels {
        pixel.swap(0, 2);
    }
    Surface::from_rgba(width, height, rgba)
}

wrap_render_handler! {
    struct RenderHandlerBuilder {
        handlers: Handlers,
    }

    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            let view = self.handlers.view.borrow();
            if let Some(rect) = rect {
                rect.width = view.width.max(1);
                rect.height = view.height.max(1);
            }
        }

        fn screen_info(&self, _browser: Option<&mut Browser>, info: Option<&mut ScreenInfo>) -> c_int {
            let Some(info) = info else { return 0 };
            info.device_scale_factor = self.handlers.view.borrow().scale;
            1
        }

        fn on_paint(
            &self,
            _browser: Option<&mut Browser>,
            type_: PaintElementType,
            dirty_rects: Option<&[Rect]>,
            buffer: *const u8,
            width: c_int,
            height: c_int,
        ) {
            let (Ok(width), Ok(height)) = (u32::try_from(width), u32::try_from(height)) else {
                return;
            };
            if buffer.is_null() || type_ != PaintElementType::VIEW {
                return;
            }
            let length = width as usize * height as usize * 4;
            // SAFETY: CEF hands us a BGRA buffer of width * height pixels that
            // stays valid for the length of this call, and we copy it out here.
            #[allow(unsafe_code)]
            let bgra = unsafe { std::slice::from_raw_parts(buffer, length) };
            if let Some(frame) = surface_from_bgra(bgra, width, height) {
                let mut view = self.handlers.view.borrow_mut();
                view.frame = Some(frame);
                view.damage
                    .extend(dirty_rects.unwrap_or_default().iter().filter_map(pixel_rect));
            }
        }
    }
}

wrap_display_handler! {
    struct DisplayHandlerBuilder {
        handlers: Handlers,
    }

    impl DisplayHandler {
        fn on_address_change(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            url: Option<&CefString>,
        ) {
            if frame.is_some_and(|frame| frame.is_main() == 1) {
                self.handlers.view.borrow_mut().address = url.map(ToString::to_string);
            }
        }

        fn on_cursor_change(
            &self,
            _browser: Option<&mut Browser>,
            _cursor: *mut u8,
            type_: CursorType,
            _custom_cursor_info: Option<&CursorInfo>,
        ) -> c_int {
            self.handlers.view.borrow_mut().pointer = Some(pointer_shape(type_));
            1
        }
    }
}

wrap_life_span_handler! {
    struct LifeSpanHandlerBuilder {
        handlers: Handlers,
    }

    impl LifeSpanHandler {
        fn on_before_popup(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: c_int,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: c_int,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut c_int>,
        ) -> c_int {
            // #lizard forgives: CEF's trait fixes this signature
            if let Some(frame) = browser.and_then(|browser| browser.main_frame()) {
                frame.load_url(target_url);
            }
            1
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            self.handlers.view.borrow_mut().closed = true;
        }
    }
}

wrap_client! {
    struct ClientBuilder {
        render: RenderHandler,
        display: DisplayHandler,
        life_span: LifeSpanHandler,
    }

    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> {
            Some(self.render.clone())
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(self.display.clone())
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(self.life_span.clone())
        }
    }
}

/// The command line switches every CEF process starts with.
#[derive(Clone)]
struct Switches {
    mute: bool,
}

wrap_app! {
    struct AppBuilder {
        switches: Switches,
    }

    impl App {
        fn on_before_command_line_processing(
            &self,
            _process_type: Option<&CefStringUtf16>,
            command_line: Option<&mut CommandLine>,
        ) {
            let Some(command_line) = command_line else { return };
            command_line.append_switch(Some(&"use-mock-keychain".into()));
            command_line.append_switch_with_value(
                Some(&"autoplay-policy".into()),
                Some(&"no-user-gesture-required".into()),
            );
            if cfg!(target_os = "linux") {
                // pages render off screen, so we need no X server or Wayland
                command_line.append_switch_with_value(
                    Some(&"ozone-platform".into()),
                    Some(&"headless".into()),
                );
                command_line.append_switch(Some(&"enable-unsafe-webgpu".into()));
                command_line.append_switch_with_value(
                    Some(&"enable-features".into()),
                    Some(&"Vulkan".into()),
                );
            }
            if self.switches.mute {
                command_line.append_switch(Some(&"mute-audio".into()));
            }
        }
    }
}

/// Whether this process is one of CEF's helpers, which the browser
/// process starts with a `--type` switch.
pub(crate) fn is_helper() -> bool {
    std::env::args().any(|argument| argument.starts_with("--type="))
}

/// The loaded CEF library, which stays loaded while this lives.
pub(crate) struct Library {
    #[cfg(target_os = "macos")]
    _loader: cef::library_loader::LibraryLoader,
}

/// Load CEF from beside this executable; the helpers load it from the
/// main bundle they live in.
pub(crate) fn load(helper: bool) -> Result<Library, Error> {
    let executable = std::env::current_exe().map_err(Error::Executable)?;
    #[cfg(target_os = "macos")]
    let library = {
        let loader = cef::library_loader::LibraryLoader::new(&executable, helper);
        if !loader.load() {
            return Err(Error::NoCef(executable));
        }
        Library { _loader: loader }
    };
    #[cfg(not(target_os = "macos"))]
    let library = {
        let _ = (helper, executable);
        Library {}
    };
    let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
    Ok(library)
}

/// Run a helper process to its end and return its exit code.
pub(crate) fn run_helper() -> i32 {
    let args = Args::new();
    cef::execute_process(
        Some(args.as_main_args()),
        None::<&mut App>,
        std::ptr::null_mut(),
    )
}

/// Where CEF keeps the profile: cookies, storage and the cache.
fn profile_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(std::env::temp_dir)
        .join("katnav")
        .join("profile")
}

/// The profile this run owns, and the lock that keeps it ours. Chromium
/// hands a URL to whichever process holds a profile and opens it there in
/// a desktop window, so a second katnav takes a fresh profile of its own.
fn claim_profile() -> (PathBuf, Option<File>) {
    let shared = profile_dir();
    let _ = std::fs::create_dir_all(&shared);
    if let Ok(lock) = File::create(shared.with_extension("lock"))
        && lock.try_lock().is_ok()
    {
        return (shared, Some(lock));
    }
    let own = std::env::temp_dir().join(format!("katnav-profile-{}", std::process::id()));
    (own, None)
}

/// CEF, initialized in the browser process.
pub(crate) struct Engine {
    _app: App,
    _profile_lock: Option<File>,
}

impl Engine {
    pub(crate) fn start(mute: bool) -> Result<Self, Error> {
        let args = Args::new();
        let mut app = AppBuilder::new(Switches { mute });
        let (profile, profile_lock) = claim_profile();
        let settings = Settings {
            windowless_rendering_enabled: 1,
            external_message_pump: 1,
            no_sandbox: 1,
            persist_session_cookies: 1,
            root_cache_path: CefString::from(profile.to_string_lossy().as_ref()),
            cache_path: CefString::from(profile.join("default").to_string_lossy().as_ref()),
            ..Default::default()
        };
        let started = cef::initialize(
            Some(args.as_main_args()),
            Some(&settings),
            Some(&mut app),
            std::ptr::null_mut(),
        );
        if started == 1 {
            Ok(Self {
                _app: app,
                _profile_lock: profile_lock,
            })
        } else {
            Err(Error::CefStart)
        }
    }

    /// Run whatever CEF has queued.
    #[allow(clippy::unused_self, reason = "only an initialized engine may pump")]
    pub(crate) fn pump(&self) {
        cef::do_message_loop_work();
    }

    /// Open `url` in a new off-screen browser that paints into `view`.
    #[allow(
        clippy::unused_self,
        reason = "only an initialized engine may open pages"
    )]
    pub(crate) fn open(&self, url: &str, view: &ViewCell) -> Result<Page, Error> {
        let handlers = Handlers {
            view: Shared::clone(view),
        };
        let mut client = ClientBuilder::new(
            RenderHandlerBuilder::new(handlers.clone()),
            DisplayHandlerBuilder::new(handlers.clone()),
            LifeSpanHandlerBuilder::new(handlers),
        );
        let window = WindowInfo {
            windowless_rendering_enabled: 1,
            ..Default::default()
        };
        let settings = BrowserSettings {
            windowless_frame_rate: 60,
            ..Default::default()
        };
        cef::browser_host_create_browser_sync(
            Some(&window),
            Some(&mut client),
            Some(&url.into()),
            Some(&settings),
            None,
            None,
        )
        .map(|browser| Page { browser })
        .ok_or(Error::CefStart)
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        cef::shutdown();
    }
}

/// One browser and the input it takes.
pub(crate) struct Page {
    browser: Browser,
}

/// A key event for CEF.
pub(crate) struct Key {
    pub(crate) kind: KeyKind,
    pub(crate) windows: i32,
    pub(crate) native: i32,
    pub(crate) character: u16,
    pub(crate) flags: u32,
}

#[derive(Clone, Copy)]
pub(crate) enum KeyKind {
    Down,
    Char,
    Up,
}

impl Page {
    fn host(&self) -> Option<cef::BrowserHost> {
        self.browser.host()
    }

    /// Tell CEF the view size or scale changed.
    pub(crate) fn resized(&self) {
        if let Some(host) = self.host() {
            host.notify_screen_info_changed();
            host.was_resized();
            host.set_focus(1);
        }
    }

    pub(crate) fn key(&self, key: &Key) {
        let event = KeyEvent {
            size: std::mem::size_of::<cef::sys::cef_key_event_t>(),
            type_: match key.kind {
                KeyKind::Down => KeyEventType::RAWKEYDOWN,
                KeyKind::Char => KeyEventType::CHAR,
                KeyKind::Up => KeyEventType::KEYUP,
            },
            modifiers: key.flags,
            windows_key_code: key.windows,
            native_key_code: key.native,
            is_system_key: 0,
            character: key.character,
            unmodified_character: key.character,
            focus_on_editable_field: 0,
        };
        if let Some(host) = self.host() {
            host.send_key_event(Some(&event));
        }
    }

    /// A button going down or up at `x`, `y` in CSS pixels.
    pub(crate) fn click(&self, at: (i32, i32), flags: u32, button: Button, up: bool, count: i32) {
        let kind = match button {
            Button::Left => MouseButtonType::LEFT,
            Button::Middle => MouseButtonType::MIDDLE,
            Button::Right => MouseButtonType::RIGHT,
            Button::Back | Button::Forward => return,
        };
        if let Some(host) = self.host() {
            host.send_mouse_click_event(Some(&mouse(at, flags)), kind, i32::from(up), count);
        }
    }

    pub(crate) fn hover(&self, at: (i32, i32), flags: u32) {
        if let Some(host) = self.host() {
            host.send_mouse_move_event(Some(&mouse(at, flags)), 0);
        }
    }

    pub(crate) fn wheel(&self, at: (i32, i32), flags: u32, (delta_x, delta_y): (i32, i32)) {
        if let Some(host) = self.host() {
            host.send_mouse_wheel_event(Some(&mouse(at, flags)), delta_x, delta_y);
        }
    }

    /// Go one page back in the history, when there is one.
    pub(crate) fn back(&self) {
        self.browser.go_back();
    }

    pub(crate) fn forward(&self) {
        self.browser.go_forward();
    }

    /// Type `text` into the focused field, as a paste does.
    pub(crate) fn insert(&self, text: &str) {
        if let Some(host) = self.host() {
            let nowhere = Range {
                from: u32::MAX,
                to: u32::MAX,
            };
            host.ime_commit_text(Some(&text.into()), Some(&nowhere), 0);
        }
    }

    /// Run `script` in the page's main frame.
    pub(crate) fn run_script(&self, script: &str) {
        if let Some(frame) = self.browser.main_frame() {
            frame.execute_java_script(Some(&script.into()), None, 0);
        }
    }

    pub(crate) fn close(&self) {
        if let Some(host) = self.host() {
            host.close_browser(1);
        }
    }
}

pub(crate) use katui::proto::input::Button;

fn mouse((x, y): (i32, i32), modifiers: u32) -> MouseEvent {
    MouseEvent { x, y, modifiers }
}

#[cfg(test)]
mod tests {
    use super::surface_from_bgra;

    #[test]
    fn bgra_frames_become_rgba() {
        let surface = surface_from_bgra(&[1, 2, 3, 255, 9, 8, 7, 255], 2, 1).expect("two pixels");
        assert_eq!(surface.pixels(), &[3, 2, 1, 255, 7, 8, 9, 255]);
    }
}

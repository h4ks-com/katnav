//! katnav: a web browser in the terminal. Headless Chromium renders the
//! page, katui puts its pixels on the screen.

mod chrome;
mod keys;
mod tab;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, OnceLock};

use katui::rt::app::run_app;
use katui::rt::events::Waker;
use katui::scene::surface::Surface;

use crate::chrome::{Browser, Event};
use crate::tab::{Message, Tab};

const FRAMES_PER_SECOND: u32 = 60;
const USAGE: &str = "usage: katnav <url or file>";

/// Everything that stops katnav from showing a page.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("found no Chrome, Chromium, Edge or Brave; set KATNAV_CHROME to the browser binary")]
    NoBrowser,
    #[error("could not start {binary}: {source}")]
    Launch {
        binary: PathBuf,
        source: std::io::Error,
    },
    #[error("could not talk to the browser: {0}")]
    Handshake(String),
    #[error(transparent)]
    Terminal(#[from] katui::rt::RtError),
}

/// What the user typed as a URL: a file that exists, a URL with a
/// scheme, or a host we prefix with https.
fn url_from_argument(argument: &str) -> String {
    let path = Path::new(argument);
    if path.exists()
        && let Ok(absolute) = path.canonicalize()
    {
        return format!("file://{}", absolute.display());
    }
    if argument.contains("://") || argument.starts_with("about:") || argument.starts_with("data:") {
        return argument.to_owned();
    }
    format!("https://{argument}")
}

fn decode(jpeg: &[u8]) -> Option<Surface> {
    let image = image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg).ok()?;
    let rgba = image.to_rgba8();
    Surface::from_rgba(rgba.width(), rgba.height(), rgba.into_raw())
}

/// Turn browser events into loop messages; frames decode here, on the
/// protocol thread, so the loop never waits on a decode.
fn forward(waker: &OnceLock<Waker<Message>>, event: Event) -> bool {
    let Some(waker) = waker.get() else {
        return true;
    };
    match event {
        Event::Frame(jpeg) => {
            decode(&jpeg).is_none_or(|frame| waker.send(Message::Frame(Arc::new(frame))))
        }
        Event::Navigated(url) => waker.send(Message::Navigated(url)),
        Event::Gone(reason) => waker.send(Message::Gone(reason)),
    }
}

fn browse(url: String) -> Result<Option<String>, Error> {
    let caps = katui::rt::tty::probe_default();
    let waker = Arc::new(OnceLock::new());
    let events = Arc::clone(&waker);
    let browser = Browser::launch(move |event| forward(&events, event))?;
    let mut tab = Tab::new(browser, caps.clone(), url, waker);
    run_app(&mut tab, std::io::stdout(), &caps, FRAMES_PER_SECOND)?;
    Ok(tab.ended().map(str::to_owned))
}

fn say(text: &str) {
    let _ = writeln!(std::io::stderr(), "{text}");
}

fn main() -> ExitCode {
    let mut arguments = std::env::args().skip(1);
    let (Some(argument), None) = (arguments.next(), arguments.next()) else {
        say(USAGE);
        return ExitCode::FAILURE;
    };
    if argument == "-h" || argument == "--help" {
        say(&format!("{USAGE}\n\nctrl-q to quit"));
        return ExitCode::SUCCESS;
    }
    match browse(url_from_argument(&argument)) {
        Ok(None) => ExitCode::SUCCESS,
        Ok(Some(reason)) => {
            say(&format!("katnav: {reason}"));
            ExitCode::FAILURE
        }
        Err(error) => {
            say(&format!("katnav: {error}"));
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::url_from_argument;

    #[test]
    fn a_bare_host_gets_https_and_a_url_stays_as_it_is() {
        assert_eq!(url_from_argument("example.com"), "https://example.com");
        assert_eq!(
            url_from_argument("http://127.0.0.1:8080/a.html"),
            "http://127.0.0.1:8080/a.html"
        );
        assert_eq!(url_from_argument("about:blank"), "about:blank");
        assert!(url_from_argument("Cargo.toml").starts_with("file:///"));
    }
}

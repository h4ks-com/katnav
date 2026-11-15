//! katnav: a web browser in the terminal. Chromium renders the page off
//! screen through CEF, katui puts its pixels on the screen.

mod engine;
mod governor;
mod keys;
mod tab;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use katui::rt::app::run_app;

use crate::engine::Engine;
use crate::tab::{FRAMES_PER_SECOND, Options, Tab};

const USAGE: &str = "usage: katnav [--game] [--mute] <url or file>

  --game  render at a lower resolution, for games
  --mute  play no sound

ctrl-q to quit";

/// Everything that stops katnav from showing a page.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("could not find katnav's own executable: {0}")]
    Executable(std::io::Error),
    #[cfg(target_os = "macos")]
    #[error("found no CEF beside {0}; run katnav from its bundle (just bundle)")]
    NoCef(PathBuf),
    #[error("CEF did not start")]
    CefStart,
    #[error(transparent)]
    Terminal(#[from] katui::rt::RtError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// What the user asked for on the command line.
#[derive(Debug, PartialEq, Eq)]
struct Request {
    url: String,
    game: bool,
    mute: bool,
}

/// The request in `arguments`, or `None` when they do not make one.
fn parse(arguments: impl IntoIterator<Item = String>) -> Option<Request> {
    let mut url = None;
    let (mut game, mut mute) = (false, false);
    for argument in arguments {
        match argument.as_str() {
            "--game" => game = true,
            "--mute" => mute = true,
            flag if flag.starts_with('-') => return None,
            _ if url.is_some() => return None,
            _ => url = Some(url_from_argument(&argument)),
        }
    }
    Some(Request {
        url: url?,
        game,
        mute,
    })
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

fn browse(request: Request) -> Result<(), Error> {
    let caps = katui::rt::tty::probe_default();
    let engine = Engine::start(request.mute)?;
    let options = Options { game: request.game };
    let mut tab = Tab::new(engine, caps.clone(), request.url, options);
    run_app(&mut tab, std::io::stdout(), &caps, FRAMES_PER_SECOND)?;
    if !tab.closed() {
        // CEF's shutdown waits for a page that never closed until its
        // watchdog kills us, so we leave without it; the helpers exit with us
        std::process::exit(0);
    }
    Ok(())
}

fn say(text: &str) {
    let _ = writeln!(std::io::stderr(), "{text}");
}

fn main() -> ExitCode {
    let helper = engine::is_helper();
    let _library = match engine::load(helper) {
        Ok(library) => library,
        Err(error) => {
            say(&format!("katnav: {error}"));
            return ExitCode::FAILURE;
        }
    };
    if helper {
        return ExitCode::from(u8::try_from(engine::run_helper()).unwrap_or(1));
    }
    let Some(request) = parse(std::env::args().skip(1)) else {
        say(USAGE);
        return ExitCode::FAILURE;
    };
    match browse(request) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            say(&format!("katnav: {error}"));
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Request, parse, url_from_argument};

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

    #[test]
    fn flags_go_anywhere_and_one_url_is_required() {
        let arguments = |list: &[&str]| {
            list.iter()
                .map(|text| (*text).to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            parse(arguments(&["--game", "example.com", "--mute"])),
            Some(Request {
                url: "https://example.com".to_owned(),
                game: true,
                mute: true,
            })
        );
        assert_eq!(parse(arguments(&["--game"])), None);
        assert_eq!(parse(arguments(&["a.com", "b.com"])), None);
        assert_eq!(parse(arguments(&["--help"])), None);
    }
}

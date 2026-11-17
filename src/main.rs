//! katnav: a web browser in the terminal. Chromium renders the page off
//! screen through CEF, katui puts its pixels on the screen.

mod bar;
#[cfg(unix)]
mod bundle;
mod drop;
mod engine;
mod governor;
mod keys;
mod pads;
mod tab;
mod trace;

use std::io::Write as _;
use std::path::Path;
use std::process::ExitCode;

use katui::rt::app::run_app;

use crate::engine::Engine;
use crate::tab::{FRAMES_PER_SECOND, Tab};

const USAGE: &str = "usage: katnav [--mute] [url or file]

  --mute     play no sound
  --version  print the version and the commit it was built from

ctrl-l type an address, alt-left go back, alt-right go forward,
ctrl-r reload, ctrl-c or ctrl-q quit";

/// Everything that stops katnav from showing a page.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("could not find katnav's own executable: {0}")]
    Executable(std::io::Error),
    #[cfg(target_os = "macos")]
    #[error("found no CEF beside {0}")]
    NoCef(std::path::PathBuf),
    #[cfg(unix)]
    #[error("CEF is gone from {0}; reinstall katnav to fetch it again")]
    CefGone(std::path::PathBuf),
    #[cfg(unix)]
    #[error("could not set up katnav beside CEF: {0}")]
    Bundle(std::io::Error),
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
    /// The page to open, or none to start at the address field.
    url: Option<String>,
    mute: bool,
}

/// The request in `arguments`, or `None` when they do not make one.
fn parse(arguments: impl IntoIterator<Item = String>) -> Option<Request> {
    let mut url = None;
    let mut mute = false;
    for argument in arguments {
        match argument.as_str() {
            "--mute" => mute = true,
            flag if flag.starts_with('-') => return None,
            _ if url.is_some() => return None,
            _ => url = Some(url_from_argument(&argument)),
        }
    }
    Some(Request { url, mute })
}

/// What the user typed as a URL: a file that exists, a URL with a
/// scheme, or a host we prefix with https.
fn url_from_argument(argument: &str) -> String {
    let path = Path::new(argument);
    if path.exists()
        && let Ok(absolute) = path.canonicalize()
    {
        return file_url(&absolute);
    }
    if argument.contains("://") || argument.starts_with("about:") || argument.starts_with("data:") {
        return argument.to_owned();
    }
    format!("https://{argument}")
}

/// The `file://` URL of an absolute path, with a third slash before a
/// windows drive letter.
fn file_url(absolute: &Path) -> String {
    let path = absolute.display().to_string();
    // windows canonicalizes to the verbatim `\\?\C:\` form, which no URL takes
    let path = path.strip_prefix(r"\\?\").unwrap_or(&path);
    let slash = if path.starts_with('/') { "" } else { "/" };
    format!("file://{slash}{path}")
}

fn browse(request: Request) -> Result<(), Error> {
    let caps = katui::rt::tty::probe_default();
    let engine = Engine::start(request.mute)?;
    let mut tab = Tab::new(engine, caps.clone(), request.url);
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

/// The release, and the commit it was built from when the build knew it.
fn version() -> String {
    let release = env!("CARGO_PKG_VERSION");
    option_env!("KATNAV_COMMIT").map_or_else(
        || format!("katnav {release}"),
        |commit| format!("katnav {release} ({commit})"),
    )
}

fn main() -> ExitCode {
    if std::env::args()
        .skip(1)
        .any(|arg| arg == "--version" || arg == "-V")
    {
        let _ = writeln!(std::io::stdout(), "{}", version());
        return ExitCode::SUCCESS;
    }
    let helper = engine::is_helper();
    #[cfg(unix)]
    if !helper && let Err(error) = bundle::run_beside_cef() {
        say(&format!("katnav: {error}"));
        return ExitCode::FAILURE;
    }
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
    use std::path::Path;

    use super::{Request, file_url, parse, url_from_argument};

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
    fn a_windows_path_makes_a_file_url() {
        assert_eq!(
            file_url(Path::new(r"\\?\C:\pages\a.html")),
            r"file:///C:\pages\a.html"
        );
        assert_eq!(file_url(Path::new("/pages/a.html")), "file:///pages/a.html");
    }

    #[test]
    fn flags_go_anywhere_and_one_url_at_most() {
        let arguments = |list: &[&str]| {
            list.iter()
                .map(|text| (*text).to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            parse(arguments(&["--mute", "example.com"])),
            Some(Request {
                url: Some("https://example.com".to_owned()),
                mute: true,
            })
        );
        assert_eq!(
            parse(arguments(&["--mute"])),
            Some(Request {
                url: None,
                mute: true,
            })
        );
        assert_eq!(parse(arguments(&["a.com", "b.com"])), None);
        assert_eq!(parse(arguments(&["--help"])), None);
    }
}

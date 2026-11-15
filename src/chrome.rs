//! Chromium as a child process, driven over the DevTools protocol on one
//! websocket: we launch it headless, attach to its page, and trade
//! commands and events with it on a thread of our own.

#![allow(
    clippy::indexing_slicing,
    reason = "serde_json reads a missing key as null, so indexing a Value never panics"
)]

use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use serde_json::{Value, json};
use tungstenite::{Message, WebSocket};

use crate::Error;

/// How long a read on the socket waits before the thread sends what we
/// queued, which bounds the latency of a key or a click.
const POLL: Duration = Duration::from_millis(4);

const LISTENING: &str = "DevTools listening on ";

/// The browsers we know how to drive, in the order we look for them.
#[cfg(target_os = "macos")]
const CANDIDATES: [&str; 4] = [
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
    "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
];
#[cfg(not(target_os = "macos"))]
const CANDIDATES: [&str; 5] = [
    "google-chrome-stable",
    "google-chrome",
    "chromium",
    "chromium-browser",
    "microsoft-edge",
];

/// The browser binary: `KATNAV_CHROME` when set, else the first one we find.
fn find_browser() -> Result<PathBuf, Error> {
    if let Some(path) = std::env::var_os("KATNAV_CHROME") {
        return Ok(PathBuf::from(path));
    }
    let on_path = |name: &str| {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|dir| dir.join(name))
            .find(|path| path.is_file())
    };
    CANDIDATES
        .iter()
        .find_map(|candidate| {
            let path = PathBuf::from(candidate);
            if path.is_absolute() {
                path.is_file().then_some(path)
            } else {
                on_path(candidate)
            }
        })
        .ok_or(Error::NoBrowser)
}

/// The profile we keep between runs, so logins and saved games survive.
fn profile_dir() -> PathBuf {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(std::env::temp_dir);
    data.join("katnav").join("profile")
}

/// Something the page told us.
#[derive(Debug)]
pub(crate) enum Event {
    /// A screencast frame as JPEG bytes.
    Frame(Vec<u8>),
    /// The page finished navigating to this URL.
    Navigated(String),
    /// The browser went away.
    Gone(String),
}

/// The running browser and the page we show.
pub(crate) struct Browser {
    child: Child,
    commands: Sender<String>,
    session: String,
    next_id: u64,
}

impl Browser {
    /// Launch the browser headless, attach to its first page and hand
    /// every page event to `on_event` from the protocol thread.
    pub(crate) fn launch(on_event: impl Fn(Event) -> bool + Send + 'static) -> Result<Self, Error> {
        let binary = find_browser()?;
        let mut child = Command::new(&binary)
            .args([
                "--headless=new",
                "--remote-debugging-port=0",
                "--no-first-run",
                "--no-default-browser-check",
                "--autoplay-policy=no-user-gesture-required",
                "--enable-unsafe-webgpu",
            ])
            .arg(format!("--user-data-dir={}", profile_dir().display()))
            .arg("about:blank")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| Error::Launch { binary, source })?;
        let endpoint = child
            .stderr
            .take()
            .ok_or(Error::Handshake("no stderr".into()))
            .and_then(read_endpoint);
        let endpoint = match endpoint {
            Ok(endpoint) => endpoint,
            Err(error) => {
                let _ = child.kill();
                return Err(error);
            }
        };
        let mut socket = connect(&endpoint)?;
        let session = attach_first_page(&mut socket)?;
        let (commands, queued) = mpsc::channel();
        std::thread::spawn(move || pump(socket, &queued, &on_event));
        Ok(Self {
            child,
            commands,
            session,
            next_id: 100,
        })
    }

    /// Send `method` with `params` to the page, ignoring the answer.
    pub(crate) fn send(&mut self, method: &str, params: Value) {
        self.next_id += 1;
        let mut message = json!({
            "id": self.next_id,
            "sessionId": self.session,
            "method": method,
        });
        message["params"] = params;
        let _ = self.commands.send(message.to_string());
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        self.send("Browser.close", json!({}));
        std::thread::sleep(Duration::from_millis(100));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The browser's websocket URL from the line it prints at startup. A
/// thread keeps draining the rest so the browser never blocks on a full pipe.
fn read_endpoint(stderr: std::process::ChildStderr) -> Result<String, Error> {
    let mut lines = BufReader::new(stderr).lines();
    let endpoint = lines
        .by_ref()
        .map_while(Result::ok)
        .find_map(|line| line.strip_prefix(LISTENING).map(str::to_owned))
        .ok_or_else(|| Error::Handshake("the browser printed no DevTools endpoint".into()))?;
    std::thread::spawn(move || lines.for_each(drop));
    Ok(endpoint)
}

fn connect(endpoint: &str) -> Result<WebSocket<TcpStream>, Error> {
    let address = endpoint
        .strip_prefix("ws://")
        .and_then(|rest| rest.split('/').next())
        .ok_or_else(|| Error::Handshake(format!("unexpected endpoint {endpoint}")))?;
    let stream =
        TcpStream::connect(address).map_err(|error| Error::Handshake(error.to_string()))?;
    stream
        .set_nodelay(true)
        .map_err(|error| Error::Handshake(error.to_string()))?;
    let (socket, _) = tungstenite::client(endpoint, stream)
        .map_err(|error| Error::Handshake(error.to_string()))?;
    Ok(socket)
}

/// Send one browser-level command and wait for its answer, before the
/// protocol thread owns the socket.
fn call(
    socket: &mut WebSocket<TcpStream>,
    id: u64,
    method: &str,
    params: &Value,
) -> Result<Value, Error> {
    let request = json!({"id": id, "method": method, "params": params});
    socket
        .send(Message::text(request.to_string()))
        .map_err(|error| Error::Handshake(error.to_string()))?;
    loop {
        let message = socket
            .read()
            .map_err(|error| Error::Handshake(error.to_string()))?;
        let Ok(text) = message.to_text() else {
            continue;
        };
        let Ok(reply) = serde_json::from_str::<Value>(text) else {
            continue;
        };
        if reply["id"].as_u64() == Some(id) {
            if let Some(error) = reply.get("error") {
                return Err(Error::Handshake(format!("{method}: {error}")));
            }
            return Ok(reply["result"].clone());
        }
    }
}

fn attach_first_page(socket: &mut WebSocket<TcpStream>) -> Result<String, Error> {
    let targets = call(socket, 1, "Target.getTargets", &json!({}))?;
    let page = targets["targetInfos"]
        .as_array()
        .and_then(|targets| targets.iter().find(|target| target["type"] == "page"))
        .and_then(|target| target["targetId"].as_str())
        .ok_or_else(|| Error::Handshake("the browser has no page".into()))?
        .to_owned();
    let attached = call(
        socket,
        2,
        "Target.attachToTarget",
        &json!({"targetId": page, "flatten": true}),
    )?;
    attached["sessionId"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::Handshake("no session".into()))
}

/// The protocol thread: send what the app queued, read what the browser
/// says, acknowledge frames at once so the next one comes.
fn pump(
    mut socket: WebSocket<TcpStream>,
    queued: &Receiver<String>,
    on_event: &impl Fn(Event) -> bool,
) {
    if let Err(error) = socket.get_mut().set_read_timeout(Some(POLL)) {
        on_event(Event::Gone(error.to_string()));
        return;
    }
    loop {
        let mut outgoing = queued.try_iter().peekable();
        let sent = outgoing.peek().is_some();
        for text in outgoing {
            let _ = socket.write(Message::text(text));
        }
        if sent && let Err(error) = socket.flush() {
            on_event(Event::Gone(error.to_string()));
            return;
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                if !dispatch(&mut socket, text.as_str(), on_event) {
                    return;
                }
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(error) => {
                on_event(Event::Gone(error.to_string()));
                return;
            }
        }
    }
}

/// Turn one protocol message into an [`Event`]; `false` once nobody listens.
fn dispatch(
    socket: &mut WebSocket<TcpStream>,
    text: &str,
    on_event: &impl Fn(Event) -> bool,
) -> bool {
    let Ok(message) = serde_json::from_str::<Value>(text) else {
        return true;
    };
    let params = &message["params"];
    match message["method"].as_str() {
        Some("Page.screencastFrame") => {
            let ack = json!({
                "id": 1,
                "sessionId": message["sessionId"],
                "method": "Page.screencastFrameAck",
                "params": {"sessionId": params["sessionId"]},
            });
            let _ = socket.send(Message::text(ack.to_string()));
            let data = params["data"].as_str().unwrap_or_default();
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data)
                .map_or(true, |jpeg| on_event(Event::Frame(jpeg)))
        }
        Some("Page.frameNavigated") if params["frame"]["parentId"].is_null() => {
            let url = params["frame"]["url"].as_str().unwrap_or_default();
            on_event(Event::Navigated(url.to_owned()))
        }
        Some("Inspector.detached" | "Target.detachedFromTarget") => {
            on_event(Event::Gone("the page closed".into()))
        }
        _ => true,
    }
}

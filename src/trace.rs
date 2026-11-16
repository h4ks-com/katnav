//! Timing lines for finding where a frame spends its time, written to the
//! file `KATNAV_TRACE` names: `KATNAV_TRACE=trace.txt just run <url>`.

use std::fs::File;
use std::io::Write as _;
use std::time::{Duration, Instant};

pub(crate) struct Trace {
    file: Option<File>,
    start: Instant,
}

impl Trace {
    pub(crate) fn from_env() -> Self {
        Self {
            file: std::env::var_os("KATNAV_TRACE").and_then(|path| File::create(path).ok()),
            start: Instant::now(),
        }
    }

    pub(crate) fn on(&self) -> bool {
        self.file.is_some()
    }

    /// One line, stamped with the milliseconds since katnav started.
    pub(crate) fn line(&mut self, text: std::fmt::Arguments<'_>) {
        let at = millis(self.start.elapsed());
        if let Some(file) = &mut self.file {
            let _ = writeln!(file, "{at:9.1} {text}");
        }
    }
}

pub(crate) fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

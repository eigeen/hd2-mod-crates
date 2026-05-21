use indicatif::MultiProgress;
use std::io::{self, Write};
use std::sync::{Mutex, OnceLock};
use tracing_subscriber::{fmt, EnvFilter};

static ACTIVE_PROGRESS: OnceLock<Mutex<Option<MultiProgress>>> = OnceLock::new();

pub fn init(verbosity: u8) {
    let default = match verbosity {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));
    let _ = fmt()
        .with_env_filter(filter)
        .with_writer(ProgressLogWriter::new)
        .try_init();
}

pub fn attach_progress(multi: MultiProgress) {
    *progress_slot().lock().expect("lock poisoned") = Some(multi);
}

pub fn detach_progress() {
    *progress_slot().lock().expect("lock poisoned") = None;
}

fn progress_slot() -> &'static Mutex<Option<MultiProgress>> {
    ACTIVE_PROGRESS.get_or_init(|| Mutex::new(None))
}

#[derive(Default)]
struct ProgressLogWriter {
    buffer: Vec<u8>,
}

impl ProgressLogWriter {
    fn new() -> Self {
        Self::default()
    }

    fn write_buffered(&mut self) -> io::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let bytes = std::mem::take(&mut self.buffer);
        let text = String::from_utf8_lossy(&bytes);
        if let Some(progress) = progress_slot().lock().expect("lock poisoned").clone() {
            if progress.is_hidden() {
                io::stderr().write_all(text.as_bytes())?;
                return Ok(());
            }
            for line in text.lines() {
                progress.println(line)?;
            }
            return Ok(());
        }
        io::stderr().write_all(text.as_bytes())
    }
}

impl Write for ProgressLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        if self.buffer.contains(&b'\n') {
            self.write_buffered()?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.write_buffered()
    }
}

impl Drop for ProgressLogWriter {
    fn drop(&mut self) {
        let _ = self.write_buffered();
    }
}

use color_eyre::eyre::Report;
use std::ffi::{OsStr, OsString};
use std::io::{self, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let should_pause = should_pause_before_exit(std::env::args_os());
    run_cli(should_pause)
}

fn run_cli(should_pause: bool) -> ExitCode {
    if let Err(error) = color_eyre::install() {
        return finish_with_error(error, should_pause);
    }

    match crate::cli::run() {
        Ok(()) => finish_successfully(should_pause),
        Err(error) => finish_with_error(error, should_pause),
    }
}

fn finish_successfully(should_pause: bool) -> ExitCode {
    pause_before_exit(should_pause);
    ExitCode::SUCCESS
}

fn finish_with_error(error: Report, should_pause: bool) -> ExitCode {
    eprintln!("{error:?}");
    pause_before_exit(should_pause);
    ExitCode::FAILURE
}

fn should_pause_before_exit(args: impl IntoIterator<Item = OsString>) -> bool {
    !args
        .into_iter()
        .skip(1)
        .any(|arg| arg == OsStr::new("--non-interactive"))
}

/// Wait for Enter so double-clicked Windows consoles keep logs visible.
fn pause_before_exit(should_pause: bool) {
    if !should_pause {
        return;
    }

    eprint!("\nPress Enter to close...");
    let _ = io::stderr().flush();

    let mut input = String::new();
    let _ = io::stdin().read_line(&mut input);
}

mod cli;

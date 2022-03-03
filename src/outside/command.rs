use std::process::{Command, Output, Stdio};

use bitflags::bitflags;
use log::{debug, trace};

use crate::result::{bail, Result};

pub const YT_DL: &str = "youtube-dl";
pub const YT_DLP: &str = "yt-dlp";
pub const FFMPEG: &str = "ffmpeg";
pub const FFXXX_DEFAULT_ARGS: [&str; 3] = ["-hide_banner", "-loglevel", "error"];

bitflags! {
    pub struct Capture: u8 {
        const STDIN = 0b0000001;
        const STDOUT = 0b0000010;
        const STDERR = 0b0000100;
    }
}

/// Run a command, returning its raw output handle.
///
/// IO handles will be captured only if the caller required it or if the log level is Debug.
/// In that last case, `stdout` and `stderr` will be logged.
///
/// The function returns an error only if the command failed to execute.
/// If the program runs but returns a non-0 status code, it will not trigger an error.
pub fn run_command<F: FnOnce(&mut Command) -> &mut Command>(
    program: &str,
    f: F,
    capture: Capture,
) -> Result<Output> {
    let is_debug = log::log_enabled!(log::Level::Debug);
    let get_io = |capture| {
        if capture {
            Stdio::piped()
        } else {
            Stdio::null()
        }
    };

    let mut cmd = Command::new(program);
    let cmd = f(&mut cmd)
        .stdin(get_io(capture.contains(Capture::STDIN)))
        .stdout(get_io(is_debug || capture.contains(Capture::STDOUT)))
        .stderr(get_io(is_debug || capture.contains(Capture::STDERR)));

    debug!("Executing command: {cmd:?}");
    let res = cmd.output()?;

    if is_debug {
        debug!("status: {}", res.status);
        debug!("stdout: {} bytes long", res.stdout.len());
        trace!("stdout: {:?}", String::from_utf8_lossy(&res.stdout));
        debug!("stderr: {} bytes long", res.stderr.len());
        trace!("stderr: {:?}", String::from_utf8_lossy(&res.stderr));
    }

    Ok(res)
}

/// Run the command and verify that it has returned a success status code.
pub fn assert_success_command<F: FnOnce(&mut Command) -> &mut Command>(
    program: &str,
    f: F,
) -> Result<()> {
    let res = run_command(program, f, Capture::empty())?;
    if res.status.success() {
        Ok(())
    } else {
        bail("Command did run but was not successful")
    }
}

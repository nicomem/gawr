use std::{
    collections::HashMap,
    ffi::OsStr,
    path::Path,
    process::{Command, Output, Stdio},
};

use anyhow::{bail, Context, Result};
use bitflags::bitflags;
use log::debug;
use regex::Regex;

use crate::types::{Metadata, Timestamp};

const YT_DLP: &str = "yt-dlp";
const FFMPEG: &str = "ffmpeg";
const FFPROBE: &str = "ffprobe";
const FFXXX_DEFAULT_ARGS: [&str; 3] = ["-hide_banner", "-loglevel", "error"];

bitflags! {
    struct Capture: u8 {
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
fn run_command<S: AsRef<str>, F: FnOnce(&mut Command) -> &mut Command>(
    program: S,
    f: F,
    capture: Capture,
) -> Result<Output> {
    let program = program.as_ref();

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
    let res = cmd.output().context("Could not run command")?;

    if is_debug {
        debug!("status: {}", res.status);
        debug!("stdout: {:?}", String::from_utf8_lossy(&res.stdout));
        debug!("stderr: {:?}", String::from_utf8_lossy(&res.stderr));
    }

    Ok(res)
}

/// Run the command and return its standard output
fn run_get_stdout<S: AsRef<str>, F: FnOnce(&mut Command) -> &mut Command>(
    program: S,
    f: F,
) -> Result<String> {
    let res = run_command(program, f, Capture::STDOUT)?;

    if res.status.success() {
        let stdout =
            String::from_utf8(res.stdout).context("Output from command is not valid UTF-8")?;
        Ok(stdout)
    } else {
        bail!("Command did run but was not successful");
    }
}

/// Run the command and return whether it has returned a success status code.
fn check_run_command<S: AsRef<str>, F: FnOnce(&mut Command) -> &mut Command>(
    program: S,
    f: F,
) -> Result<bool> {
    let res = run_command(program, f, Capture::empty())?;
    Ok(res.status.success())
}

/// Run the command and verify that it has returned a success status code.
fn assert_success_command<S: AsRef<str>, F: FnOnce(&mut Command) -> &mut Command>(
    program: S,
    f: F,
) -> Result<()> {
    if check_run_command(program, f)? {
        Ok(())
    } else {
        bail!("Command did run but was not successful.")
    }
}

pub fn get_playlist_videos_id(playlist_id: &str) -> Result<Vec<String>> {
    let output = run_get_stdout(YT_DLP, |cmd| {
        cmd.arg("-q")
            .arg("--flat-playlist")
            .args(["--print", "%(id)s"])
            .arg("--")
            .arg(playlist_id)
    })?;

    Ok(output.split_whitespace().map(String::from).collect())
}

pub fn download_audio_with_meta<P: AsRef<Path>>(path: P, video_id: &str) -> Result<bool> {
    check_run_command(YT_DLP, |cmd| {
        cmd.arg("-q")
            .args([OsStr::new("-o"), path.as_ref().as_os_str()])
            .args(["-f", "bestaudio"])
            .arg("--add-metadata")
            // 2 lines below to force setting the video title & uploader (https://github.com/yt-dlp/yt-dlp/issues/904)
            .args(["--parse-metadata", "%(title)s:%(meta_title)s"])
            .args(["--parse-metadata", "%(uploader)s:%(meta_artist)s"])
            .arg("--")
            .arg(video_id)
    })
}

pub fn extract_metadata<P: AsRef<Path>>(path: P) -> Result<Metadata> {
    let output = run_get_stdout(FFPROBE, |cmd| {
        cmd.args(FFXXX_DEFAULT_ARGS)
            .arg(path.as_ref().as_os_str())
            .args(["-of", "json"])
            .arg("-show_format")
    })?;

    let json: serde_json::Value =
        serde_json::from_str(&output).context("Could not parse JSON output")?;

    let json = json
        .as_object()
        .unwrap()
        .get("format")
        .unwrap()
        .as_object()
        .unwrap();

    // Extract the duration tp add it to the tags
    let duration = json.get("duration").unwrap().as_str().unwrap();
    let tags = json.get("tags").unwrap().as_object().unwrap();

    let mut map: HashMap<String, String> = tags
        .into_iter()
        .flat_map(|(k, v)| v.as_str().map(|s| (k.to_lowercase(), s.to_owned())))
        .collect();
    map.insert("duration".to_owned(), duration.to_owned());

    Ok(Metadata::new(map))
}

pub fn extract_clip<P1: AsRef<Path>, P2: AsRef<Path>>(
    input: P1,
    output: P2,
    start: &Timestamp,
    end: Option<&Timestamp>,
    album: &str,
) -> Result<()> {
    assert_success_command(FFMPEG, |cmd| {
        let mut cmd = cmd
            .args(FFXXX_DEFAULT_ARGS)
            .arg("-y")
            .args([OsStr::new("-i"), input.as_ref().as_os_str()])
            .args(["-map_metadata", "-1"])
            .args(["-metadata", &format!("album={album}")])
            .args(["-ss", &start.t_start]);

        if let Some(end) = end {
            cmd = cmd.args(["-to", &end.t_start])
        }

        cmd.args(["-c:a", "copy"]).arg("--").arg(output.as_ref())
    })
}

pub fn normalize_audio<P1: AsRef<Path>, P2: AsRef<Path>>(input: P1, output: P2) -> Result<()> {
    // First pass to generate the statistics
    let input = input.as_ref().as_os_str();
    let res = run_command(
        FFMPEG,
        |cmd| {
            // Do not use FFXXX_DEFAULT_ARGS as it would remove the wanted output
            cmd.arg("-hide_banner")
                .arg("-y")
                .args([OsStr::new("-i"), input])
                .args(["-pass", "1"])
                .args(["-filter:a", "loudnorm=print_format=json"])
                .args(["-f", "null", "-"])
        },
        Capture::STDERR,
    )?;

    // Wanted output is in stderr along with other things, so we need to parse it
    // Fortunately, the wanted part is at the end and "easily" findable
    let stderr = String::from_utf8_lossy(&res.stderr);

    // Take the lines in reverse until we find "{"
    let mut json_parts: Vec<&str> = stderr
        .lines()
        .rev()
        .take_while(|&line| line != "{")
        .collect();
    // Re-add the "{"
    json_parts.push("{");
    // Put back the lines in the correct order
    json_parts.reverse();
    // Join the lines together
    let json_str: String = json_parts.join("\n");

    let json = serde_json::from_str::<serde_json::Value>(&json_str)
        .context("Could not parse JSON output")?;
    let json = json.as_object().context("JSON output is not an object")?;

    let get_str = |k: &str| -> Result<&str> {
        json.get(k)
            .with_context(|| format!("Key {k} not found in JSON object"))?
            .as_str()
            .with_context(|| format!("Value of key {k} is not a string"))
    };

    let input_i = get_str("input_i")?;
    let input_lra = get_str("input_lra")?;
    let input_tp = get_str("input_tp")?;
    let input_thresh = get_str("input_thresh")?;

    // Second pass to apply the normalization using the previous statistics
    let filter = format!(
        "loudnorm=linear=true:\
        measured_I={input_i}:\
        measured_LRA={input_lra}:\
        measured_tp={input_tp}:\
        measured_thresh={input_thresh}"
    );

    let output = output.as_ref().as_os_str();
    assert_success_command(FFMPEG, |cmd| {
        cmd.args(FFXXX_DEFAULT_ARGS)
            .arg("-y")
            .args([OsStr::new("-i"), input])
            .args(["-pass", "2"])
            .args(["-filter:a", &filter])
            .args(["-c:a", "libopus", "-b:a", "128K"])
            .arg(output)
    })
}

pub fn get_file_duration<P: AsRef<Path>>(path: P) -> Result<u64> {
    let res = run_get_stdout(FFPROBE, |cmd| {
        cmd.args(["-show_entries", "format=duration"])
            .arg(path.as_ref().as_os_str())
    })?;

    // The duration ffprobe returns is a float but we don't need the decimals
    let re = Regex::new(r"duration=(\d+)")?;
    let cap = re
        .captures(&res)
        .context("Did not find the duration in the ffprobe output")?;

    let duration = cap.get(1).unwrap().as_str();
    duration.parse().context("Could not parse duration")
}

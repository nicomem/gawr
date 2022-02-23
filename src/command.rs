use std::{collections::HashMap, ffi::OsStr, path::Path, process::Command};

use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;

use crate::types::{Metadata, Timestamp};

const YT_DLP: &str = "yt-dlp";
const FFMPEG: &str = "ffmpeg";
const FFPROBE: &str = "ffprobe";
const FFXXX_DEFAULT_ARGS: [&str; 3] = ["-hide_banner", "-loglevel", "error"];
const FFMPEG_NORMALIZE: &str = "ffmpeg-normalize";

fn run_command<S: AsRef<str>, F: FnOnce(&mut Command) -> &mut Command>(
    program: S,
    f: F,
) -> Result<String> {
    let program = program.as_ref();
    let res = f(&mut Command::new(program))
        .output()
        .with_context(|| format!("Could not run {} command", program))?;

    if res.status.success() {
        let stdout =
            String::from_utf8(res.stdout).context("Output from command is not valid UTF-8")?;
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&res.stderr);
        bail!("{program} did run but was not successful. Here is its stderr: {stderr}")
    }
}

fn check_run_command<S: AsRef<str>, F: FnOnce(&mut Command) -> &mut Command>(
    program: S,
    f: F,
) -> Result<bool> {
    let program = program.as_ref();
    let res = f(&mut Command::new(program))
        .status()
        .with_context(|| format!("Could not run {} command", program))?;

    Ok(res.success())
}

fn assert_success_command<S: AsRef<str>, F: FnOnce(&mut Command) -> &mut Command>(
    program: S,
    f: F,
) -> Result<()> {
    let program = program.as_ref();
    let res = f(&mut Command::new(program))
        .output()
        .with_context(|| format!("Could not run {} command", program))?;

    if res.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&res.stderr);
        let stdout = String::from_utf8_lossy(&res.stdout);
        Err(anyhow!(
            "{program} did run but was not successful. Here is its stderr: {stderr}; and stdout: {stdout}"
        ))
    }
}

pub fn get_playlist_videos_id(playlist_id: &str) -> Result<Vec<String>> {
    let output = run_command(YT_DLP, |cmd| {
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
            .args(["--audio-format", "opus"])
            .arg("--add-metadata")
            // 2 lines below to force setting the video title & uploader (https://github.com/yt-dlp/yt-dlp/issues/904)
            .args(["--parse-metadata", "%(title)s:%(meta_title)s"])
            .args(["--parse-metadata", "%(uploader)s:%(meta_artist)s"])
            .arg("--")
            .arg(video_id)
    })
}

pub fn extract_metadata<P: AsRef<Path>>(path: P) -> Result<Metadata> {
    let output = run_command(FFPROBE, |cmd| {
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

pub fn normalize_audio<P: AsRef<Path>>(path: P) -> Result<()> {
    assert_success_command(FFMPEG_NORMALIZE, |cmd| {
        cmd.arg(path.as_ref().as_os_str())
            .args([OsStr::new("-o"), path.as_ref().as_os_str()])
            .arg("-f")
            .args(["-c:a", "libopus"])
            .args(["-b:a", "128K"])
    })
}

pub fn get_file_duration<P: AsRef<Path>>(path: P) -> Result<u64> {
    let res = run_command(FFPROBE, |cmd| {
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

use std::{
    ffi::OsStr,
    path::Path,
    process::{Command, Output},
};

use anyhow::Context;

use super::command::{assert_success_command, run_command, Capture, YT_DL, YT_DLP};
use crate::{
    result::{bail, Error, Result},
    types::Metadata,
};

/// Interface for downloading streams and their metadata
pub trait StreamDownloader {
    /// Get the playlist's videos IDs.
    ///
    /// The given id could refer to either a playlist ID or a video ID.
    /// If it refers to a video ID, simply return the video ID.
    ///
    /// If the ID correspond to both a playlist and a video ID,
    /// the implementation is allowed to choose either code path.
    fn get_playlist_videos_id(&self, id: &str) -> Result<Vec<String>>;

    /// Get the video metadata
    fn get_metadata(&self, video_id: &str) -> Result<Metadata>;

    /// Download the audio stream of the video with the corresponding ID.
    fn download_audio<P: AsRef<Path>>(&self, path: P, video_id: &str) -> Result<()>;
}

/// Interface for the [youtube-dl](https://github.com/ytdl-org/youtube-dl) program
pub struct Ytdl {
    program: &'static str,
}

impl Ytdl {
    /// Verify that the `yt-dlp` or `youtube-dl` binaries are reachable
    pub fn new() -> Result<Self> {
        // Check `yt-dlp`
        if assert_success_command(YT_DLP, |cmd| cmd.arg("--version")).is_ok() {
            Ok(Self { program: YT_DLP })
        } else if assert_success_command(YT_DL, |cmd| cmd.arg("--version")).is_ok() {
            // Check `youtube-dl`
            Ok(Self { program: YT_DL })
        } else {
            bail("Neither yt-dl not youtube-dl found")
        }
    }

    /// Run the command and check if it failed with saying the stream is unavailable.
    /// In that case, return [`Error::UnavailableStream`].
    ///
    /// In other cases, return the output handle.
    pub fn run_check_availability<F>(&self, f: F, capture: Capture) -> Result<Output>
    where
        F: FnOnce(&mut Command) -> &mut Command,
    {
        let res = run_command(self.program, f, capture | Capture::STDERR)?;

        let stderr = String::from_utf8_lossy(&res.stderr);
        let is_unavailable = stderr
            .lines()
            .any(|line| line.starts_with("ERROR:") && line.to_lowercase().contains("unavailable"));
        if is_unavailable {
            Err(Error::UnavailableStream)
        } else {
            Ok(res)
        }
    }
}

impl StreamDownloader for Ytdl {
    fn get_playlist_videos_id(&self, id: &str) -> Result<Vec<String>> {
        let res = self.run_check_availability(
            |cmd| {
                cmd.arg("-q")
                    .arg("--flat-playlist")
                    .arg("--get-id")
                    .arg("--")
                    .arg(id)
            },
            Capture::STDOUT,
        )?;

        let output = String::from_utf8_lossy(&res.stdout);
        Ok(output.split_whitespace().map(String::from).collect())
    }

    fn get_metadata(&self, video_id: &str) -> Result<Metadata> {
        let res = self.run_check_availability(
            |cmd| {
                cmd.arg("-q")
                    .arg("--skip-download")
                    .arg("-j")
                    .arg("--")
                    .arg(video_id)
            },
            Capture::STDOUT,
        )?;
        let output = String::from_utf8_lossy(&res.stdout);

        let json =
            serde_json::from_str::<serde_json::Value>(&output).context("Could not parse json")?;
        let json = json.as_object().context("JSON is not an object")?;

        let get_key = |key| -> Result<String> {
            Ok(json
                .get(key)
                .with_context(|| format!("Key '{key}' not found in JSON"))?
                .as_str()
                .with_context(|| format!("Value of key '{key}' is not a string"))?
                .to_owned())
        };

        let duration = json
            .get("duration")
            .context("Key 'duration' not found in JSON")?
            .as_u64()
            .context("Value of key 'duration' is not a u64")?;

        Ok(Metadata {
            title: get_key("title")?,
            uploader: get_key("uploader")?,
            description: get_key("description")?,
            duration,
        })
    }

    fn download_audio<P: AsRef<Path>>(&self, path: P, video_id: &str) -> Result<()> {
        let res = self.run_check_availability(
            |cmd| {
                cmd.arg("-q")
                    .args([OsStr::new("-o"), path.as_ref().as_os_str()])
                    .arg("--no-continue") // Or else fails when file already exists, even an empty one
                    .args(["-f", "bestaudio"])
                    .arg("--add-metadata")
                    // 2 lines below to force setting the video title & uploader (https://github.com/yt-dlp/yt-dlp/issues/904)
                    .args(["--parse-metadata", "%(title)s:%(meta_title)s"])
                    .args(["--parse-metadata", "%(uploader)s:%(meta_artist)s"])
                    .arg("--")
                    .arg(video_id)
            },
            Capture::empty(),
        )?;

        if res.status.success() {
            Ok(())
        } else {
            bail("Command did run but was not successful")
        }
    }
}

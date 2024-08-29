use std::{
    ffi::OsStr,
    fmt::Debug,
    path::Path,
    process::{Command, Output},
};

use miette::{miette, Context, IntoDiagnostic};
use simd_json::base::ValueAsScalar;

use super::command::{assert_success_command, run_command, Capture, YT_DL, YT_DLP};
use crate::{
    result::{Error, Result},
    types::Metadata,
};

/// A list of characters that may cause problems to other programs
const PROBLEMATIC_CHARS: &[char] = &[
    '"', '\'', '/', '\\', '|', '~', '$', '#', ':', '*', '<', '>', '?', ',',
];

/// Interface for downloading streams and their metadata
pub trait StreamDownloader: Sync + Debug {
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
    fn download_audio(&self, path: &Path, video_id: &str) -> Result<()>;
}

/// Interface for the [youtube-dl](https://github.com/ytdl-org/youtube-dl) program
#[derive(Debug)]
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
            Err(miette!("Neither yt-dl not youtube-dl found").into())
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
        let is_unavailable = stderr.lines().any(|line| {
            if !line.starts_with("ERROR:") {
                return false;
            }
            let line = line.to_lowercase();
            line.contains("private") || line.contains("unavailable")
        });
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
        let mut res = self.run_check_availability(
            |cmd| {
                cmd.arg("-q")
                    .arg("--skip-download")
                    .arg("-j")
                    .arg("--")
                    .arg(video_id)
            },
            Capture::STDOUT,
        )?;

        let simd_json::BorrowedValue::Object(json) = simd_json::to_borrowed_value(&mut res.stdout)
            .into_diagnostic()
            .wrap_err("Could not parse json")?
        else {
            return Err(miette!("JSON is not an object").into());
        };

        let get_key = |k| -> Result<&str> {
            let simd_json::BorrowedValue::String(str) = json
                .get(k)
                .ok_or_else(|| miette!(format!("Key {k} not found in JSON object")))?
            else {
                return Err(miette!(format!("Value of key {k} is not a string")).into());
            };
            Ok(str)
        };

        // Remove potentially problematic characters from the title
        let title = get_key("title")?;
        let title = title
            .split(PROBLEMATIC_CHARS)
            .map(str::trim)
            .collect::<Vec<_>>()
            .join(" ");

        let simd_json::BorrowedValue::Static(duration) = json
            .get("duration")
            .ok_or_else(|| miette!("Key 'duration' not found in JSON"))?
        else {
            return Err(miette!("Value of key 'duration' is not a u64").into());
        };

        let duration = duration
            .as_u64()
            .ok_or_else(|| miette!("Value of key 'duration' is not a u64"))?;

        Ok(Metadata {
            title,
            duration,
            uploader: get_key("uploader")?.to_string(),
            description: get_key("description")?.to_string(),
        })
    }

    fn download_audio(&self, path: &Path, video_id: &str) -> Result<()> {
        let res = self.run_check_availability(
            |cmd| {
                cmd.arg("-q")
                    .args([OsStr::new("-o"), path.as_os_str()])
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
            Err(miette!("Command did run but was not successful").into())
        }
    }
}

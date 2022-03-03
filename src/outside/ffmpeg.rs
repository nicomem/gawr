use std::{ffi::OsStr, fmt::Debug, path::Path};

use anyhow::Context;

use crate::{result::Result, types::Timestamp};

use super::command::{assert_success_command, run_command, Capture, FFMPEG, FFXXX_DEFAULT_ARGS};

pub trait StreamTransformer: Sync + Debug {
    /// Extract a clip containing the stream data between the two
    /// timestamps from the input file to the output file.
    ///
    /// If the end timestamp is not specified, the clip should
    /// continue until the end of the stream.
    fn extract_clip(
        &self,
        input: &Path,
        output: &Path,
        start: &Timestamp,
        end: Option<&Timestamp>,
        album: &str, // TODO: This is weird, refactor to have better API
    ) -> Result<()>;

    /// Normalize an audio stream
    fn normalize_audio(&self, input: &Path, output: &Path) -> Result<()>;
}

/// Interface for the [ffprobe](https://ffmpeg.org) program
#[derive(Debug)]
pub struct Ffmpeg;

impl Ffmpeg {
    /// Verify that the `ffmpeg` binary is reachable
    pub fn new() -> Result<Self> {
        assert_success_command(FFMPEG, |cmd| cmd.arg("-version"))?;

        Ok(Self)
    }
}

impl StreamTransformer for Ffmpeg {
    fn extract_clip(
        &self,
        input: &Path,
        output: &Path,
        start: &Timestamp,
        end: Option<&Timestamp>,
        album: &str,
    ) -> Result<()> {
        assert_success_command(FFMPEG, |cmd| {
            let mut cmd = cmd
                .args(FFXXX_DEFAULT_ARGS)
                .arg("-y")
                .args([OsStr::new("-i"), input.as_os_str()])
                .args(["-map_metadata", "-1"])
                .args(["-metadata", &format!("album={album}")])
                .args(["-ss", &start.t_start]);

            if let Some(end) = end {
                cmd = cmd.args(["-to", &end.t_start])
            }

            cmd.args(["-c:a", "copy"]).arg("--").arg(output)
        })
    }

    fn normalize_audio(&self, input: &Path, output: &Path) -> Result<()> {
        // First pass to generate the statistics
        let input = input.as_os_str();
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
            Ok(json
                .get(k)
                .with_context(|| format!("Key {k} not found in JSON object"))?
                .as_str()
                .with_context(|| format!("Value of key {k} is not a string"))?)
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

        let output = output.as_os_str();
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
}

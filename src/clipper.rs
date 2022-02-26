use std::{
    hint::unreachable_unchecked,
    path::{Path, PathBuf},
};

use anyhow::Context;
use log::debug;
use rayon_core::{ThreadPool, ThreadPoolBuilder};
use regex::Regex;

use crate::{
    io::{build_output_path, named_tempfile, touch},
    outside::StreamTransformer,
    result::{bail, Result},
    types::{Extension, Timestamp, Timestamps},
    Metadata,
};

pub struct Clipper<'a> {
    stream_tsf: &'a dyn StreamTransformer,
    pool: ThreadPool,
}

impl<'a> Clipper<'a> {
    pub fn new(stream_tsf: &'a dyn StreamTransformer) -> Result<Self> {
        Ok(Self {
            stream_tsf,
            pool: ThreadPoolBuilder::new().build()?,
        })
    }

    /// Try to extract the timestamps from the description.
    pub fn extract_timestamps(&self, description: &str, clip_regex: &Regex) -> Result<Timestamps> {
        let timestamps = Timestamps::extract_timestamps(description, clip_regex);
        if timestamps.len() > 5 {
            Ok(timestamps)
        } else {
            bail(
                "Nope, too little timestamps, something went wrong. \
                Description: {description}. Timestamps: {timestamps:?}",
            )
        }
    }

    /// Verify that the file stream duration is longer than the latest timestamp.
    /// If there is a timestamp after the stream end, it would mean that the file
    /// download stopped before completing.
    pub fn is_file_complete(&self, stream_duration: u64, timestamps: &Timestamps) -> Result<bool> {
        // The minimum number of second the last clip must last for the stream to be considered complete
        const MIN_CLIP_LENGTH: u64 = 10;

        let last_timestamp = timestamps.last().unwrap();
        let last_secs = Timestamp::to_seconds(&last_timestamp.t_start);

        Ok(last_secs + MIN_CLIP_LENGTH < stream_duration)
    }

    pub fn create_clips(
        &self,
        input: &Path,
        out_dir: &Path,
        timestamps: &Timestamps,
        metadata: &Metadata,
        video_id: &str,
        extension: &str,
    ) {
        let album = format!("{} ({})", metadata.title, video_id);
        let preprocess = |start: &Timestamp, _| {
            let output = build_output_path(&out_dir, &start.title, extension)
                .context("Could not build output file path")
                .unwrap();

            touch(&output).unwrap();

            output
        };

        let process = |start, end, output: PathBuf| {
            self.create_clip(input, &output, start, end, &album)
                .context("Could not create clip")
                .unwrap();
        };

        self.for_each_clip(timestamps, preprocess, &process);
    }

    /// Execture a process for every clip described by the timestamps concurrently.
    ///
    /// This function does not create any clip itself but simply call the process function with
    /// each starting and ending timestamps.
    ///
    /// Before the process function is sent to run concurrently for each clip,
    /// another preprocessing function is called in the main thread.
    /// It is garanteed that this preprocessing function cannot be executed concurrently.
    fn for_each_clip<'scope, T, Fpre, F>(
        &self,
        timestamps: &'scope Timestamps,
        mut preprocess: Fpre,
        process: F,
    ) where
        T: Send + 'scope,
        Fpre: FnMut(&'scope Timestamp, Option<&'scope Timestamp>) -> T + Send,
        F: Fn(&'scope Timestamp, Option<&'scope Timestamp>, T) + Sync,
    {
        let process = &process;
        self.pool.scope(move |scope| {
            for sl in timestamps.windows(2) {
                if let [start, end] = sl {
                    let data = preprocess(start, Some(end));
                    scope.spawn(move |_| process(start, Some(end), data));
                } else {
                    // SAFETY: slice::windows(2) generates slices of 2 elements
                    // In the future, the use of slice::array_windows would remove the test
                    unsafe { unreachable_unchecked() }
                }
            }

            // Do not forget to do the last timestamp up to the end of the stream
            if let Some(start) = timestamps.last() {
                let data = preprocess(start, None);
                scope.spawn(move |_| process(start, None, data));
            }
        });
    }

    /// Create a clip of a stream.
    ///
    /// `input` stream will be cut to keep only data from timestamps `start` to `end`
    /// and will be saved to `output`. The `album` metadata will be added to the file.
    ///
    /// If `end` is not specified, clip will continue until the end of the stream.
    fn create_clip(
        &self,
        input: &Path,
        output: &Path,
        start: &Timestamp,
        end: Option<&Timestamp>,
        album: &str,
    ) -> Result<()> {
        // Create a temporary file with the correct extension
        let out_ext = Extension::from_path(output).context("Invalid output extension")?;
        let tmp = named_tempfile(out_ext)?;

        self.stream_tsf
            .extract_clip(input, tmp.path(), start, end, album)
            .context("Could not extract a clip of the audio file from the timestamps")?;

        self.stream_tsf
            .normalize_audio(tmp.path(), output)
            .context("Could not normalize audio")?;

        debug!(
            "Clip '{}' ({} - {}) completed",
            start.title,
            start.t_start,
            end.map_or("END", |end| end.t_start.as_str())
        );

        Ok(())
    }
}

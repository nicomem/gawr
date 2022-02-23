use std::{
    hint::unreachable_unchecked,
    path::{Path, PathBuf},
};

use log::debug;
use rayon::{ThreadPool, ThreadPoolBuilder};

use anyhow::{bail, Context, Result};

use crate::{
    command::{extract_clip, extract_metadata, get_file_duration, normalize_audio},
    io::{build_output_path, touch},
    types::{Timestamp, Timestamps},
    Metadata,
};

pub struct Clipper {
    pool: ThreadPool,
}

impl Clipper {
    // TODO
    pub fn new() -> Result<Self> {
        Ok(Self {
            pool: ThreadPoolBuilder::new().build()?,
        })
    }

    /// Try to extract the metadata and timestamps from the file.
    /// Return an error if an operation failed.
    /// Return Ok(None) if the operation succeded
    pub fn extract_metadata_timestamps<P: AsRef<Path>>(path: P) -> Result<(Metadata, Timestamps)> {
        let path = path.as_ref();
        let metadata =
            extract_metadata(path).context("Could not extract description of downloaded file")?;

        let description = &metadata["description"];
        let timestamps = Timestamps::extract_timestamps(description);

        if timestamps.len() < 5 {
            bail!(
                "Nope, too little timestamps, something went wrong. Description: {description}. Timestamps: {timestamps:?}"
            );
        }

        Ok((metadata, timestamps))
    }

    /// Verify that the file stream duration is longer than the latest timestamp.
    /// If there is a timestamp after the stream end, it would mean that the file
    /// download stopped before completing.
    pub fn is_file_complete<P: AsRef<Path>>(path: P, timestamps: &Timestamps) -> Result<bool> {
        // The minimum number of second the last clip must last for the stream to be considered complete
        const MIN_CLIP_LENGTH: u64 = 10;

        let last_timestamp = timestamps.last().unwrap();
        let last_secs = Timestamp::to_seconds(&last_timestamp.t_start);

        let file_duration = get_file_duration(path).context("Could not get file duration")?;

        Ok(last_secs + MIN_CLIP_LENGTH < file_duration)
    }

    pub fn create_clips<P1: AsRef<Path>, P2: AsRef<Path>>(
        &self,
        input: P1,
        out_dir: P2,
        timestamps: &Timestamps,
        metadata: &Metadata,
        video_id: &str,
        extension: &str,
    ) {
        let out_dir = out_dir.as_ref();
        let input = input.as_ref();
        let album = format!(
            "{} ({})",
            metadata.get("title").map_or("???", |s| s),
            video_id
        );

        let preprocess = |start: &Timestamp, _| {
            let output = build_output_path(&out_dir, &start.title, extension)
                .context("Could not build output file path")
                .unwrap();

            touch(&output)
                .with_context(|| format!("Could not create the empty file {}", output.display()))
                .unwrap();

            output
        };

        let process =
            |start, end, output: PathBuf| Clipper::create_clip(input, output, start, end, &album);

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
    fn create_clip<P1: AsRef<Path>, P2: AsRef<Path>>(
        input: P1,
        output: P2,
        start: &Timestamp,
        end: Option<&Timestamp>,
        album: &str,
    ) {
        let output = output.as_ref();

        extract_clip(input, output, start, end, album)
            .context("Could not extract a clip of the audio file from the timestamps")
            .unwrap();

        normalize_audio(output)
            .context("Could not normalize audio")
            .unwrap();

        debug!(
            "Clip number '{}' ({} - {}) completed",
            start.title,
            start.t_start,
            end.map_or("END", |end| end.t_start.as_str())
        );
    }
}

use std::{
    hint::unreachable_unchecked,
    path::{Path, PathBuf},
    sync::{
        mpsc::{Receiver, SyncSender},
        Arc, Mutex,
    },
};

use anyhow::Context;
use log::{debug, info};
use rayon_core::{ThreadPool, ThreadPoolBuilder};

use crate::{
    already_processed::AlreadyProcessed,
    io::{build_output_path, named_tempfile, touch},
    outside::StreamTransformer,
    result::{err_msg, Result},
    types::{Extension, Metadata, Timestamp, Timestamps},
};

use super::{Actor, DownloadedStream, VideoTitle};

pub struct ClipperActor<'a> {
    pool: ThreadPool,
    stream_tsf: &'a dyn StreamTransformer,
    out_dir: &'a Path,
    ext: Extension,
    cache: Arc<Mutex<AlreadyProcessed>>,

    receive_channel: Option<Receiver<DownloadedStream>>,
    send_channel: Option<SyncSender<VideoTitle>>,
}

impl Actor<DownloadedStream, VideoTitle> for ClipperActor<'_> {
    fn set_receive_channel(&mut self, channel: Receiver<DownloadedStream>) {
        self.receive_channel = Some(channel);
    }

    fn set_send_channel(&mut self, channel: SyncSender<VideoTitle>) {
        self.send_channel = Some(channel);
    }

    fn run(mut self) -> Result<()> {
        let receive_channel = self
            .receive_channel
            .take()
            .ok_or_else(|| err_msg("Receive channel not set"))?;

        let _send_channel = self
            .send_channel
            .take()
            .ok_or_else(|| err_msg("Send channel not set"))?;

        debug!("Actor started, waiting for a downloaded stream");

        for DownloadedStream {
            video_id,
            file,
            metadata,
            timestamps,
        } in receive_channel
        {
            debug!("Stream '{video_id}' received");
            info!(
                "Splitting '{}' into {} clips",
                metadata.title,
                timestamps.len()
            );

            self.create_clips(
                file.path(),
                self.out_dir,
                &timestamps,
                &metadata,
                &video_id,
                self.ext,
            );

            let mut cache = self.cache.lock().unwrap();
            cache.push(video_id)?;

            debug!("Iteration completed. Waiting for next stream");
        }

        info!("All iterations completed. Stopping the actor.");
        Ok(())
    }
}

impl<'a> ClipperActor<'a> {
    pub fn new(
        num_threads: usize,
        stream_tsf: &'a dyn StreamTransformer,
        out_dir: &'a Path,
        ext: Extension,
        cache: Arc<Mutex<AlreadyProcessed>>,
    ) -> Result<Self> {
        let pool = ThreadPoolBuilder::new().num_threads(num_threads).build()?;
        Ok(Self {
            pool,
            stream_tsf,
            out_dir,
            ext,
            cache,
            receive_channel: None,
            send_channel: None,
        })
    }

    fn create_clips(
        &self,
        input: &Path,
        out_dir: &Path,
        timestamps: &Timestamps,
        metadata: &Metadata,
        video_id: &str,
        extension: Extension,
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
            let out_tmp = named_tempfile(extension)
                .context("Could not create tempfile")
                .unwrap();

            // Create clip to tempfile (slow, things may go bad)
            Self::create_clip(self.stream_tsf, input, out_tmp.path(), start, end, &album)
                .context("Could not create clip")
                .unwrap();

            // When finished, move to output file (fast, nearly no errors)
            // First try to do a simple move
            if std::fs::rename(&out_tmp, &output).is_err() {
                debug!("Moving file failed, falling back to copying");
                std::fs::copy(&out_tmp, &output).unwrap();
            }
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
        stream_tsf: &'a dyn StreamTransformer,
        input: &Path,
        output: &Path,
        start: &Timestamp,
        end: Option<&Timestamp>,
        album: &str,
    ) -> Result<()> {
        // Create a temporary file with the correct extension
        let out_ext = Extension::from_path(output).context("Invalid output extension")?;
        let tmp = named_tempfile(out_ext)?;

        stream_tsf
            .extract_clip(input, tmp.path(), start, end, album)
            .context("Could not extract a clip of the audio file from the timestamps")?;

        stream_tsf
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

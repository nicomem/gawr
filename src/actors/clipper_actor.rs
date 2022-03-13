use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::Context;
use crossbeam_channel::{Receiver, Sender};
use log::{debug, info};
use once_cell::sync::OnceCell;

use crate::{
    already_processed::AlreadyProcessed,
    io::{find_unused_prefix, named_tempfile, touch},
    outside::StreamTransformer,
    result::{err_msg, Result},
    types::{Extension, Timestamp},
    utils::MutexUtils,
};

use super::{Actor, TimestampedClip, VideoTitle};

#[derive(Debug)]
pub struct ClipperActor<'a> {
    id: usize,
    stream_tsf: &'a dyn StreamTransformer,
    out_dir: &'a Path,
    ext: Extension,
    cache: Arc<Mutex<AlreadyProcessed>>,

    receive_channel: Option<Receiver<TimestampedClip>>,
    send_channel: Option<Sender<VideoTitle>>,
}

impl Actor<TimestampedClip, VideoTitle> for ClipperActor<'_> {
    fn set_receive_channel(&mut self, channel: Receiver<TimestampedClip>) {
        self.receive_channel = Some(channel);
    }

    fn set_send_channel(&mut self, channel: Sender<VideoTitle>) {
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

        debug!(
            "{}: Actor started, waiting for a downloaded stream",
            self.id
        );

        for TimestampedClip {
            stream_info,
            start,
            end,
        } in receive_channel
        {
            let video_id = &stream_info.video_id;
            let stream_file = &stream_info.stream_file;
            let metadata = &stream_info.metadata;

            debug!("{}: Stream '{video_id}' received", self.id);
            info!(
                "{}: Clipping '{}' ({} - {}) into '{}'",
                self.id,
                metadata.title,
                start.t_start,
                end.as_ref().map_or("END", |end| end.t_start.as_str()),
                start.title
            );

            let out_empty = self.reserve_output_path(self.out_dir, &start.title, self.ext);

            let out_tmp = named_tempfile(self.ext)
                .context("Could not create tempfile")
                .unwrap();

            // Create clip to tempfile (slow, things may go bad)
            let album = format!("{} ({})", metadata.title, video_id);
            self.create_clip(
                stream_file.path(),
                out_tmp.path(),
                &start,
                end.as_ref(),
                &album,
            )
            .context("Could not create clip")
            .unwrap();

            let output = out_empty.with_extension(self.ext.with_no_dot());

            // When finished, move to output file (fast, nearly no errors)
            // First try to do a simple move
            if std::fs::rename(&out_tmp, &output).is_err() {
                debug!("Moving file failed, falling back to copying");
                std::fs::copy(&out_tmp, &output).unwrap();
            }

            // Remove the placeholder
            std::fs::remove_file(out_empty).unwrap();

            info!("{}: Clip '{}' completed", self.id, start.title);

            // If last clip processed, add video_id to cache
            if Arc::strong_count(&stream_info) == 1 {
                self.cache
                    .with_lock(|mut cache| cache.push(video_id.to_string()))?;
            }

            debug!("{}: Iteration completed. Waiting for next clip", self.id);
        }

        debug!("{}: All iterations completed. Stopping the actor.", self.id);
        Ok(())
    }
}

impl<'a> ClipperActor<'a> {
    pub fn new(
        id: usize,
        stream_tsf: &'a dyn StreamTransformer,
        out_dir: &'a Path,
        ext: Extension,
        cache: Arc<Mutex<AlreadyProcessed>>,
    ) -> Result<Self> {
        Ok(Self {
            id,
            stream_tsf,
            out_dir,
            ext,
            cache,
            receive_channel: None,
            send_channel: None,
        })
    }

    /// Create an empty placeholder for the clip in the output directory.
    ///
    /// This will return a path to the placeholder with a ".empty" extension
    /// such that when replacing the extension with the given one, it can be
    /// created without worry of overwriting a file.
    ///
    /// Uses internally a lock to avoid returning the same path in two concurrent
    /// method calls.
    /// This however assumes that the output directory is not changing outside
    /// of this method during the call.
    fn reserve_output_path(&self, out_dir: &Path, title: &str, extension: Extension) -> PathBuf {
        static LOCK: OnceCell<Mutex<()>> = OnceCell::new();

        LOCK.get_or_init(|| Mutex::new(())).with_lock(|_lock| {
            let mut output = find_unused_prefix(out_dir, title, extension, true)
                .context("Could not build output file path")
                .unwrap();

            // Use the .empty extension for the placeholder
            output.set_extension("empty");

            touch(&output).unwrap();

            output
        })
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

        Ok(())
    }
}

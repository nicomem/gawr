use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crossbeam_channel::{Receiver, Sender};
use miette::{miette, Context, IntoDiagnostic, Result};
use once_cell::sync::OnceCell;
use tracing::{debug, info, warn};

use crate::{
    database::{CacheDb, Sqlite},
    io::{find_unused_prefix, named_tempfile, touch},
    outside::StreamTransformer,
    types::{Bitrate, Extension, Timestamp},
    utils::MutexUtils,
};

use super::{Actor, TimestampedClip, VideoTitle};

#[derive(Debug)]
pub struct ClipperActor<'a> {
    id: usize,
    stream_tsf: &'a dyn StreamTransformer,
    out_dir: &'a Path,
    ext: Extension,
    cache: &'a Sqlite,
    bitrate: Bitrate,

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
            .ok_or_else(|| miette!("Receive channel not set"))?;

        let _send_channel = self
            .send_channel
            .take()
            .ok_or_else(|| miette!("Send channel not set"))?;

        if self.id == 0 {
            self.delete_empty_files()
                .wrap_err("Could not delete empty files")?;
        }

        debug!("Actor started, waiting for a downloaded stream");

        for TimestampedClip {
            stream_info,
            start,
            end,
            clip_idx,
        } in receive_channel
        {
            let video_id = &stream_info.video_id;
            let stream_file = &stream_info.stream_file;
            let metadata = &stream_info.metadata;

            debug!("Stream '{}' received", video_id);
            if end.is_none() && metadata.title == start.title {
                info!("Clipping '{}' entire stream into one file", metadata.title);
            } else {
                info!(
                    "Clipping '{}' ({} - {}) into '{}'",
                    metadata.title,
                    start.t_start,
                    end.as_ref().map_or("END", |end| end.t_start.as_str()),
                    start.title
                );
            }

            let out_empty = Self::reserve_output_path(self.out_dir, &start.title, self.ext);
            let out_tmp = named_tempfile(self.ext).wrap_err("Could not create tempfile")?;

            // Create clip to tempfile (slow, things may go bad)
            let album = format!("{} ({})", metadata.title, video_id);
            self.create_clip(
                stream_file.path(),
                out_tmp.path(),
                &start,
                end.as_ref(),
                &album,
            )
            .wrap_err("Could not create clip")?;

            let output = out_empty.with_extension(self.ext.with_no_dot());

            // When finished, move to output file (fast, nearly no errors)
            // First try to do a simple move
            if std::fs::rename(&out_tmp, &output).is_err() {
                debug!("Moving file failed, falling back to copying");
                std::fs::copy(&out_tmp, &output).unwrap();
            }

            self.cache.complete_work(stream_info.db_id, clip_idx)?;

            // Remove the placeholder
            std::fs::remove_file(out_empty).unwrap();

            info!("Clip '{}' completed", start.title);

            // If last clip processed, add video_id to cache
            if Arc::strong_count(&stream_info) == 1 {
                self.cache.set_video_as_completed(stream_info.db_id)?;
            }

            debug!("Iteration completed. Waiting for next clip");
        }

        debug!("All iterations completed. Stopping the actor");
        Ok(())
    }
}

impl<'a> ClipperActor<'a> {
    pub fn new(
        id: usize,
        stream_tsf: &'a dyn StreamTransformer,
        out_dir: &'a Path,
        ext: Extension,
        cache: &'a Sqlite,
        bitrate: Bitrate,
    ) -> Self {
        Self {
            id,
            stream_tsf,
            out_dir,
            ext,
            cache,
            bitrate,
            receive_channel: None,
            send_channel: None,
        }
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
    fn reserve_output_path(out_dir: &Path, title: &str, extension: Extension) -> PathBuf {
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
        let out_ext =
            Extension::from_path(output).ok_or_else(|| miette!("Invalid output extension"))?;
        let tmp = named_tempfile(out_ext)?;

        self.stream_tsf
            .extract_clip(input, tmp.path(), start, end, album)
            .wrap_err("Could not extract a clip of the audio file from the timestamps")?;

        self.stream_tsf
            .normalize_audio(tmp.path(), output, self.bitrate)
            .wrap_err("Could not normalize audio")?;

        Ok(())
    }

    /// Delete every file with the "empty" extension in the output directory
    fn delete_empty_files(&self) -> Result<()> {
        for entry in self
            .out_dir
            .read_dir()
            .into_diagnostic()
            .wrap_err("Could not read output directory")?
        {
            let entry = entry.into_diagnostic()?;
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if path.is_file() && ext.eq_ignore_ascii_case("empty") {
                    if let Err(err) = std::fs::remove_file(&path) {
                        warn!("Could not remove file '{}': {}", path.display(), err);
                    }
                }
            }
        }
        Ok(())
    }
}

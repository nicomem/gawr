use std::{
    path::Path,
    sync::{
        mpsc::{Receiver, SyncSender},
        Arc, Mutex,
    },
};

use anyhow::Context;
use log::{debug, error, info, trace, warn};
use regex::Regex;

use crate::{
    already_processed::AlreadyProcessed,
    io::named_tempfile,
    outside::StreamDownloader,
    result::{err_msg, Error, Result},
    types::{Extension, Metadata, Timestamp, Timestamps},
    utils::MutexUtils,
};

use super::{
    message::{DownloadedStream, VideoId},
    Actor,
};

#[derive(Debug)]
pub struct DownloadActor<'a> {
    stream_dl: &'a dyn StreamDownloader,
    skip_timestamps: bool,
    clip_regex: &'a [Regex],
    cache: Arc<Mutex<AlreadyProcessed>>,

    receive_channel: Option<Receiver<VideoId>>,
    send_channel: Option<SyncSender<DownloadedStream>>,
}

impl Actor<VideoId, DownloadedStream> for DownloadActor<'_> {
    fn set_receive_channel(&mut self, channel: Receiver<VideoId>) {
        self.receive_channel = Some(channel);
    }

    fn set_send_channel(&mut self, channel: SyncSender<DownloadedStream>) {
        self.send_channel = Some(channel);
    }

    fn run(mut self) -> Result<()> {
        let receive_channel = self
            .receive_channel
            .take()
            .ok_or_else(|| err_msg("Receive channel not set"))?;

        let send_channel = self
            .send_channel
            .take()
            .ok_or_else(|| err_msg("Send channel not set"))?;

        debug!("Actor started, waiting for a video ID");

        for video_id in receive_channel {
            debug!("Video ID '{video_id}' received");

            if self.cache.with_lock(|cache| cache.contains(&video_id)) {
                debug!("Video already processed. Skipping it");
                continue;
            }

            // Put mkv as the stream file format as:
            // - Not giving any will cause an error (even though it may write another file format)
            // - It should accept any kind of audio format
            // With that, the stream data should be copied as-is, without modification
            let output = named_tempfile(Extension::Mkv)?;

            let (metadata, timestamps) =
                match self.download_and_extract_metadata(&video_id, output.path()) {
                    Ok(res) => res,
                    Err(Error::UnavailableStream) => {
                        error!(
                            "Video {video_id} is unavailable. \
                            Not downloaded but still added in cache"
                        );
                        self.cache
                            .with_lock(|mut cache| cache.push(video_id.to_string()))?;
                        continue;
                    }
                    err => err.context("Could not download and extract metadata and timestamps")?,
                };

            debug!("title       = {}", metadata.title);
            debug!("uploader    = {}", metadata.uploader);
            debug!("duration    = {}", metadata.duration);
            debug!("description = {} bytes long", metadata.description.len());
            trace!("description = {}", metadata.description);

            send_channel
                .send(DownloadedStream {
                    video_id,
                    file: output,
                    metadata,
                    timestamps,
                })
                .unwrap();

            debug!("Iteration completed. Waiting for next video ID");
        }

        debug!("All iterations completed. Stopping the actor.");
        Ok(())
    }
}

impl<'a> DownloadActor<'a> {
    pub fn new(
        stream_dl: &'a dyn StreamDownloader,
        skip_timestamps: bool,
        clip_regex: &'a [Regex],
        cache: Arc<Mutex<AlreadyProcessed>>,
    ) -> Self {
        Self {
            stream_dl,
            skip_timestamps,
            clip_regex,
            cache,
            receive_channel: None,
            send_channel: None,
        }
    }

    fn download_and_extract_metadata(
        &self,
        video_id: &str,
        out: &Path,
    ) -> Result<(Metadata, Timestamps)> {
        let metadata = self.stream_dl.get_metadata(video_id)?;

        loop {
            info!("Downloading video {video_id}");
            self.stream_dl.download_audio(out, video_id)?;

            let mut timestamps = if self.skip_timestamps {
                info!("Downloaded file, skip timestamps extraction");

                Timestamps::new(vec![])
            } else {
                info!("Downloaded file, extracting timestamps");

                let timestamps =
                    Timestamps::extract_timestamps(&metadata.description, self.clip_regex);

                debug!("Timestamps: {}", timestamps);
                if !Self::is_file_complete(metadata.duration, &timestamps)? {
                    warn!("Downloaded file seems incomplete. Retry downloading it again");
                    continue;
                }

                timestamps
            };

            if timestamps.is_empty() {
                debug!("No timestamp. Clipping the entire video");
                let start = Timestamp {
                    t_start: "00:00".to_string(),
                    title: metadata.title.to_string(),
                };

                timestamps = Timestamps::new(vec![start])
            }

            return Ok((metadata, timestamps));
        }
    }

    /// Verify that the file stream duration is longer than the latest timestamp.
    ///
    /// If there is a timestamp after the stream end, it would mean that the file
    /// download stopped before completing.
    ///
    /// If there is no timestamp, return true.
    fn is_file_complete(stream_duration: u64, timestamps: &Timestamps) -> Result<bool> {
        // The minimum number of second the last clip must last for the stream to be considered complete
        const MIN_CLIP_LENGTH: u64 = 10;

        if let Some(last_timestamp) = timestamps.last() {
            let last_secs = Timestamp::to_seconds(&last_timestamp.t_start);

            Ok(last_secs + MIN_CLIP_LENGTH < stream_duration)
        } else {
            Ok(true)
        }
    }
}

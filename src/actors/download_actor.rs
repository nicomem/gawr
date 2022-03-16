use std::path::Path;

use crossbeam_channel::{Receiver, Sender};
use log::{debug, error, info, trace, warn};
use miette::{miette, Context, IntoDiagnostic, Result};
use regex::Regex;

use crate::{
    database::{CacheDb, ProcessedState, Sqlite},
    io::named_tempfile,
    outside::StreamDownloader,
    types::{Extension, Metadata, Timestamp, Timestamps},
};

use super::{Actor, DownloadedStream, VideoId};

#[derive(Debug)]
pub struct DownloadActor<'a> {
    stream_dl: &'a dyn StreamDownloader,
    skip_timestamps: bool,
    clip_regex: &'a [Regex],
    cache: &'a Sqlite,

    receive_channel: Option<Receiver<VideoId>>,
    send_channel: Option<Sender<DownloadedStream>>,
}

impl Actor<VideoId, DownloadedStream> for DownloadActor<'_> {
    fn set_receive_channel(&mut self, channel: Receiver<VideoId>) {
        self.receive_channel = Some(channel);
    }

    fn set_send_channel(&mut self, channel: Sender<DownloadedStream>) {
        self.send_channel = Some(channel);
    }

    fn run(mut self) -> Result<()> {
        let receive_channel = self
            .receive_channel
            .take()
            .ok_or_else(|| miette!("Receive channel not set"))?;

        let send_channel = self
            .send_channel
            .take()
            .ok_or_else(|| miette!("Send channel not set"))?;

        debug!("Actor started, waiting for a video ID");

        for video_id in receive_channel {
            debug!("Video ID '{video_id}' received");

            let (db_id, video_state) = self.cache.check_video(&video_id)?;
            if video_state == ProcessedState::Completed {
                debug!("Video already processed. Skipping it");
                continue;
            }

            // Put mkv as the stream file format as:
            // - Not giving any will cause an error (even though it may write another file format)
            // - It should accept any kind of audio format
            // With that, the stream data should be copied as-is, without modification
            let stream_file = named_tempfile(Extension::Mkv)?;

            let (metadata, timestamps) = match self
                .download_and_extract_metadata(&video_id, stream_file.path())
            {
                Ok(res) => res,
                Err(crate::result::Error::UnavailableStream) => {
                    error!(
                        "Video {video_id} is unavailable. \
                            Not downloaded but still added in cache"
                    );
                    self.cache.set_video_as_completed(db_id)?;
                    continue;
                }
                Err(crate::result::Error::Miette(report)) => {
                    Err(report.wrap_err("Could not download and extract metadata and timestamps"))?
                }
            };

            debug!("title       = {}", metadata.title);
            debug!("uploader    = {}", metadata.uploader);
            debug!("duration    = {}", metadata.duration);
            debug!("description = {} bytes long", metadata.description.len());
            trace!("description = {}", metadata.description);

            send_channel
                .send(DownloadedStream {
                    video_id,
                    file: stream_file,
                    metadata,
                    timestamps,
                    db_id,
                    video_state,
                })
                .into_diagnostic()
                .wrap_err("Could not send message")?;

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
        cache: &'a Sqlite,
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
    ) -> crate::result::Result<(Metadata, Timestamps)> {
        let metadata = self
            .stream_dl
            .get_metadata(video_id)
            .map_err(|err| err.wrap_err_with(|| "Could not get stream metadata"))?;

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

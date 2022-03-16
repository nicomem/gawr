use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender};
use log::{debug, info};
use miette::{miette, Context, IntoDiagnostic, Result};

use crate::{
    actors::StreamInfo,
    database::{self, CacheDb, ProcessedState, Sqlite},
};

use super::{Actor, DownloadedStream, TimestampedClip};

/// Simple actor whose job is only to dispatch a downloaded stream's
/// timestamped clips to the next actor.
///
/// This enables the previous actor to directly download the next video
/// instead of waiting that the next actor has received the last clip.
pub struct TimestampActor<'a> {
    cache: &'a Sqlite,

    receive_channel: Option<Receiver<DownloadedStream>>,
    send_channel: Option<Sender<TimestampedClip>>,
}

impl Actor<DownloadedStream, TimestampedClip> for TimestampActor<'_> {
    fn set_receive_channel(&mut self, channel: Receiver<DownloadedStream>) {
        self.receive_channel = Some(channel);
    }

    fn set_send_channel(&mut self, channel: Sender<TimestampedClip>) {
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

        for DownloadedStream {
            video_id,
            file,
            metadata,
            timestamps,
            db_id,
            video_state,
        } in receive_channel
        {
            let work_indexes: Vec<database::ClipIdx> = match video_state {
                ProcessedState::NotProcessed => {
                    self.cache
                        .assign_work(db_id, timestamps.len() as _)
                        .wrap_err("Could not assign work")?;
                    (0..timestamps.len()).map(|n| n as _).collect()
                }
                ProcessedState::RemainingClips(v) => v,
                ProcessedState::ProcessedClips(v) => (0..timestamps.len())
                    .map(|n| n as _)
                    .filter(|n| !v.contains(n))
                    .collect(),
                ProcessedState::Completed => unimplemented!(),
            };

            if work_indexes.is_empty() {
                // No work but was not marked as complete
                debug!("Video db_id {db_id} has no work but was not marked as completed");
                self.cache
                    .set_video_as_completed(db_id)
                    .wrap_err_with(|| format!("Could not set video db_id {db_id} as completed"))?;
                continue;
            }

            if work_indexes.len() != timestamps.len() {
                // Pending work
                info!(
                    "Resuming work on '{}': {}/{} remaining",
                    &metadata.title,
                    work_indexes.len(),
                    timestamps.len()
                );
            }

            let stream_info = Arc::new(StreamInfo {
                video_id,
                stream_file: file,
                metadata,
                db_id,
            });

            // Send every timestamped clip
            for clip_idx in work_indexes {
                let start = timestamps[clip_idx as usize].clone();
                let end = timestamps.get(clip_idx as usize + 1).cloned();
                send_channel
                    .send(TimestampedClip {
                        stream_info: stream_info.clone(),
                        start,
                        end,
                        clip_idx,
                    })
                    .into_diagnostic()
                    .wrap_err("Could not send message")?;
            }

            debug!("Iteration completed. Waiting for next stream");
        }

        debug!("All iterations completed. Stopping the actor.");
        Ok(())
    }
}

impl<'a> TimestampActor<'a> {
    pub fn new(cache: &'a Sqlite) -> Self {
        Self {
            cache,
            receive_channel: None,
            send_channel: None,
        }
    }
}

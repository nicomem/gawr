use std::{hint::unreachable_unchecked, sync::Arc};

use crossbeam_channel::{Receiver, Sender};
use log::debug;

use crate::{actors::StreamInfo, result::err_msg};

use super::{Actor, DownloadedStream, TimestampedClip};

/// Simple actor whose job is only to dispatch a downloaded stream's
/// timestamped clips to the next actor.
///
/// This enables the previous actor to directly download the next video
/// instead of waiting that the next actor has received the last clip.
pub struct TimestampActor {
    receive_channel: Option<Receiver<DownloadedStream>>,
    send_channel: Option<Sender<TimestampedClip>>,
}

impl Actor<DownloadedStream, TimestampedClip> for TimestampActor {
    fn set_receive_channel(&mut self, channel: Receiver<DownloadedStream>) {
        self.receive_channel = Some(channel);
    }

    fn set_send_channel(&mut self, channel: Sender<TimestampedClip>) {
        self.send_channel = Some(channel);
    }

    fn run(mut self) -> crate::result::Result<()> {
        let receive_channel = self
            .receive_channel
            .take()
            .ok_or_else(|| err_msg("Receive channel not set"))?;

        let send_channel = self
            .send_channel
            .take()
            .ok_or_else(|| err_msg("Send channel not set"))?;

        debug!("Actor started, waiting for a video ID");

        for DownloadedStream {
            video_id,
            file,
            metadata,
            timestamps,
        } in receive_channel
        {
            let stream_info = Arc::new(StreamInfo {
                video_id,
                stream_file: file,
                metadata,
            });

            // Send every timestamped clip
            for sl in timestamps.windows(2) {
                if let [start, end] = sl {
                    send_channel
                        .send(TimestampedClip {
                            stream_info: stream_info.clone(),
                            start: start.clone(),
                            end: Some(end.clone()),
                        })
                        .unwrap();
                } else {
                    // SAFETY: slice::windows(2) generates slices of 2 elements
                    // In the future, the use of slice::array_windows would remove the test
                    unsafe { unreachable_unchecked() }
                }
            }

            // Do not forget to do the last timestamp up to the end of the stream
            if let Some(start) = timestamps.last() {
                send_channel
                    .send(TimestampedClip {
                        stream_info,
                        start: start.clone(),
                        end: None,
                    })
                    .unwrap();
            }
            debug!("Iteration completed. Waiting for next stream");
        }

        debug!("All iterations completed. Stopping the actor.");
        Ok(())
    }
}

impl TimestampActor {
    pub fn new() -> Self {
        Self {
            receive_channel: None,
            send_channel: None,
        }
    }
}

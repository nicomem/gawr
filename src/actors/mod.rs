use std::sync::mpsc::{Receiver, SyncSender};

mod clipper_actor;
mod download_actor;
mod message;

pub use clipper_actor::ClipperActor;
pub use download_actor::DownloadActor;
pub use message::*;

use crate::result::Result;

pub trait Actor<From, To> {
    fn set_receive_channel(&mut self, channel: Receiver<From>);

    fn set_send_channel(&mut self, channel: SyncSender<To>);

    fn run(self) -> Result<()>;
}

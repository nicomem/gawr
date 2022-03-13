mod clipper_actor;
mod download_actor;
mod message;
mod timestamp_actor;

pub use clipper_actor::ClipperActor;
use crossbeam_channel::{Receiver, Sender};
pub use download_actor::DownloadActor;
pub use message::*;
pub use timestamp_actor::TimestampActor;

use crate::result::Result;

pub trait Actor<From, To> {
    fn set_receive_channel(&mut self, channel: Receiver<From>);

    fn set_send_channel(&mut self, channel: Sender<To>);

    fn run(self) -> Result<()>;
}

pub fn connect_actors<From, Shared, To>(
    from: &mut dyn Actor<From, Shared>,
    to: &mut dyn Actor<Shared, To>,
    (send, receive): (Sender<Shared>, Receiver<Shared>),
) {
    from.set_send_channel(send);
    to.set_receive_channel(receive);
}

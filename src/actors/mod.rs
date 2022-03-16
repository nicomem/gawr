mod clipper_actor;
mod download_actor;
mod message;
mod timestamp_actor;

use crossbeam_channel::{Receiver, Sender};
use miette::Result;

pub use clipper_actor::ClipperActor;
pub use download_actor::DownloadActor;
pub use message::*;
pub use timestamp_actor::TimestampActor;

/// A trait for implementing the Actor design pattern.
///
/// An object implementing this trait can receive messages, process them and send back messages to the next actor.
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

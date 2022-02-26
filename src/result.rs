use std::borrow::Cow;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("The stream is not available")]
    UnavailableStream,

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Rayon(#[from] rayon_core::ThreadPoolBuildError),

    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),

    #[error("{0}")]
    Msg(Cow<'static, str>),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

pub fn bail<T>(msg: impl Into<Cow<'static, str>>) -> Result<T> {
    Err(Error::Msg(msg.into()))
}

pub type Result<T> = std::result::Result<T, Error>;

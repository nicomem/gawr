use std::fmt::Display;

use miette::miette;

#[derive(Debug)]
pub enum Error {
    UnavailableStream,

    Miette(miette::Report),
}

impl From<miette::Report> for Error {
    fn from(err: miette::Report) -> Self {
        Error::Miette(err)
    }
}

impl From<Error> for miette::Report {
    fn from(err: Error) -> Self {
        match err {
            Error::UnavailableStream => miette!("Unavailable stream"),
            Error::Miette(err) => err,
        }
    }
}

impl Error {
    pub fn wrap_err_with<D, F>(self, f: F) -> Error
    where
        D: Display + Send + Sync + 'static,
        F: FnOnce() -> D,
    {
        match self {
            Error::Miette(report) => Error::Miette(report.wrap_err(f())),
            err => err,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

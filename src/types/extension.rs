use std::{ffi::OsStr, path::Path};

use clap::{builder::PossibleValue, ValueEnum};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Extension {
    Mka,
    Mkv,
    Ogg,
    Webm,
}

impl ValueEnum for Extension {
    fn value_variants<'a>() -> &'a [Self] {
        &[
            Extension::Mka,
            Extension::Mkv,
            Extension::Ogg,
            Extension::Webm,
        ]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(match self {
            Extension::Mka => PossibleValue::new("mka"),
            Extension::Mkv => PossibleValue::new("mkv"),
            Extension::Ogg => PossibleValue::new("ogg"),
            Extension::Webm => PossibleValue::new("webm"),
        })
    }
}

impl Extension {
    /// Return the extension with the leading dot.
    /// e.g. ".ext"
    pub fn with_dot(self) -> &'static str {
        match self {
            Self::Mka => ".mka",
            Self::Mkv => ".mkv",
            Self::Ogg => ".ogg",
            Self::Webm => ".webm",
        }
    }

    /// Return the extension without the leading dot.
    /// e.g. "ext"
    pub fn with_no_dot(self) -> &'static str {
        match self {
            Self::Mka => "mka",
            Self::Mkv => "mkv",
            Self::Ogg => "ogg",
            Self::Webm => "webm",
        }
    }

    /// Parse the raw extension string, stripped of its prefix dot
    pub fn from_no_dot(ext: &str) -> Option<Self> {
        match ext {
            "mka" => Some(Self::Mka),
            "mkv" => Some(Self::Mkv),
            "ogg" => Some(Self::Ogg),
            "webm" => Some(Self::Webm),
            _ => None,
        }
    }

    /// Parse the path file extension.
    /// Return None in case of no or invalid extension.
    pub fn from_path(path: &Path) -> Option<Self> {
        path.extension()
            .and_then(OsStr::to_str)
            .and_then(Self::from_no_dot)
    }
}

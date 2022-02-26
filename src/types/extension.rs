use std::path::Path;

use clap::ArgEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ArgEnum)]
pub enum Extension {
    Mka,
    Mkv,
    Ogg,
    Webm,
}

impl Extension {
    /// Return the extension with the leading dot.
    /// e.g. ".ext"
    pub fn with_dot(self) -> &'static str {
        match self {
            Extension::Mka => ".mka",
            Extension::Mkv => ".mkv",
            Extension::Ogg => ".ogg",
            Extension::Webm => ".webm",
        }
    }

    /// Parse the path file extension.
    /// Return None in case of no or invalid extension.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Option<Self> {
        path.as_ref()
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| match ext {
                "mka" => Some(Self::Mka),
                "mkv" => Some(Self::Mkv),
                "ogg" => Some(Self::Ogg),
                "webm" => Some(Self::Webm),
                _ => None,
            })
    }
}

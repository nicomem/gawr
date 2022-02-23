use std::path::Path;

use clap::ArgEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ArgEnum)]
pub enum Extension {
    Ogg,
    Mkv,
}

impl Extension {
    /// Return the extension with the leading dot.
    /// e.g. ".ext"
    pub fn with_dot(self) -> &'static str {
        match self {
            Extension::Ogg => ".ogg",
            Extension::Mkv => ".mkv",
        }
    }

    /// Parse the path file extension.
    /// Return None in case of no or invalid extension.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Option<Self> {
        path.as_ref()
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| match ext {
                "ogg" => Some(Self::Ogg),
                "mkv" => Some(Self::Mkv),
                _ => None,
            })
    }
}

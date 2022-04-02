use std::{fmt::Display, str::FromStr};

#[derive(Debug, Clone, Copy)]
pub struct Bitrate(u16);

impl FromStr for Bitrate {
    type Err = Box<dyn std::error::Error + Sync + Send>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(num_prefix) = s.to_lowercase().strip_suffix('k') {
            Ok(Self(num_prefix.parse()?))
        } else {
            Err(Box::from("Bitrate does not end with 'K'"))
        }
    }
}

impl Display for Bitrate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}K", self.0)
    }
}

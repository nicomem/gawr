use std::path::PathBuf;

use clap::{ArgEnum, Parser};

use crate::types::Extension;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ArgEnum)]
pub enum Split {
    Full,
    Clips,
}

macro_rules! arg_env {
    ($v:literal) => {
        concat!("GAWR_", $v)
    };
}

#[derive(Parser, Debug)]
pub struct Args {
    /// The playlist ID of all videos to download
    #[clap(long, env=arg_env!("PLAYLIST"))]
    pub playlist: String,

    /// The path to the output directory
    #[clap(long, env=arg_env!("OUT"))]
    pub out: PathBuf,

    /// The path to the cache file, avoiding processing multiple times the same videos
    #[clap(long, env=arg_env!("CACHE"))]
    pub cache: PathBuf,

    /// Path to the temporary file. If already existant, content will be lost.
    /// It must have a verified valid extension (same possible values as --ext)
    #[clap(long, env=arg_env!("TMP"))]
    pub tmp: PathBuf,

    /// Either keep the entire video or create clips based on timestamps in the description
    #[clap(long, arg_enum, env=arg_env!("SPLIT"))]
    pub split: Split,

    /// The file extension to use for the output files. Defines the file container format to use
    #[clap(long, arg_enum, env=arg_env!("EXT"))]
    pub ext: Extension,
}

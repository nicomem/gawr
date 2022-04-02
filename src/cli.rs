use std::path::PathBuf;

use clap::{ArgEnum, Parser};
use regex::Regex;

use crate::types::{Bitrate, Extension};

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

/// Wrapper-tool around `youtube-dl` to create an audio library out of web videos.
/// Download, clip, and normalize audio streams.
#[derive(Parser, Debug)]
#[clap(author, version, about)]
pub struct Args {
    /// The playlist ID of all videos to download
    /// or the ID of the single video to download
    #[clap(env=arg_env!("ID"))]
    pub id: String,

    /// The path to the output directory
    #[clap(long, env=arg_env!("OUT"))]
    pub out: PathBuf,

    /// The path to the cache file, avoiding processing multiple times the same videos
    #[clap(long, env=arg_env!("CACHE"))]
    pub cache: PathBuf,

    /// Either keep the entire video or create clips based on timestamps in the description
    #[clap(long, arg_enum, env=arg_env!("SPLIT"))]
    pub split: Split,

    /// The file extension to use for the output files. Defines the file container format to use
    #[clap(long, arg_enum, default_value_t=Extension::Ogg, env=arg_env!("EXT"))]
    pub ext: Extension,

    /// The regular expressions for extracting clip timestamps from the description.
    /// The default value should be able to detect and parse most timestamps.
    ///
    /// Must have two named captured groups: `time` and `title`,
    /// corresponding to the starting timestamp and the title of the clip.
    ///
    /// The option can be set multiple times, resulting in multiple patterns.
    /// For every line in the description, every pattern will be tested until one matches.
    ///
    /// If at least one pattern is specified, the default patterns will not be tested.
    ///
    /// Must use the [Regex crate syntax](https://docs.rs/regex/latest/regex/#syntax)
    #[clap(long, env=arg_env!("CLIP_REGEX"))]
    pub clip_regex: Option<Vec<Regex>>,

    /// Randomize the order in which the videos are downloaded.
    /// Do not influence how clips are processed.
    #[clap(long, env=arg_env!("SHUFFLE"))]
    pub shuffle: bool,

    /// Assume that the machine has this number of cores.
    /// This can be used to increase or decrease the number of worker threads spawned.
    ///
    /// When using a value of 0, it will determine automatically the number of cores from the system.
    #[clap(long, default_value_t=0, env=arg_env!("CORES"))]
    pub cores: usize,

    /// The logging level to use
    #[clap(long, default_value_t=tracing::Level::INFO, env=arg_env!("LOG"))]
    pub log: tracing::Level,

    /// The audio bitrate to use for output files.
    /// Must follow the `ffmpeg` bitrate format.
    #[clap(long, default_value="96K", env=arg_env!("BITRATE"))]
    pub bitrate: Bitrate,
}

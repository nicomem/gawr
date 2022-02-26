mod already_processed;
mod cli;
mod clipper;
mod file_counter;
mod io;
mod outside;
mod result;
mod types;

use std::path::Path;

use anyhow::Context;
use clap::Parser;
use cli::Split;
use log::{debug, error, info, warn};
use outside::{Ffmpeg, StreamDownloader, StreamTransformer, Ytdl};
use regex::Regex;
use result::Error::UnavailableStream;
use types::{Metadata, Timestamps};

use crate::{
    already_processed::AlreadyProcessed,
    cli::Args,
    clipper::Clipper,
    file_counter::FileCounter,
    io::named_tempfile,
    result::Result,
    types::{Extension, Timestamp},
};

fn main() -> anyhow::Result<()> {
    // Initialize the environment & CLI
    dotenv::dotenv().ok();
    env_logger::init();
    let args = Args::parse();

    // Make sure the output directory is created
    std::fs::create_dir_all(&args.out)?;

    let (stream_dl, stream_tsf) = load_external_components(&args)?;
    let (mut cache, mut file_counter, clipper) = load_internal_components(&args, &stream_tsf)?;

    // Download the playlist videos id
    info!("Get the playlist videos id");
    let videos_id = stream_dl
        .get_playlist_videos_id(&args.id)
        .context("Could not get playlist videos id")?;
    info!("{} videos in the playlist", videos_id.len());

    let initial_count = file_counter.count();
    let skip_timestamps = matches!(args.split, Split::Full);
    for video_id in videos_id {
        if cache.contains(&video_id) {
            debug!("Video {video_id} already processed. Skipping it");
            continue;
        }

        // Put mkv as the stream file format as:
        // - Not giving any will cause an error (even though it may write another file format)
        // - It should accept any kind of audio format
        // With that, the stream data should be copied as-is, without modification
        let tmp = named_tempfile(Extension::Mkv)?;

        let (metadata, timestamps) = match download_and_extract_metadata(
            &clipper,
            &stream_dl,
            &video_id,
            tmp.path(),
            skip_timestamps,
            &args.clip_regex,
        ) {
            Ok(res) => res,
            Err(UnavailableStream) => {
                error!("Video {video_id} is unavailable. Not downloaded but still added in cache");
                cache.push(video_id)?;
                continue;
            }
            err => err.context("Could not download and extract metadata and timestamps")?,
        };

        debug!("metadata = {metadata:?}");
        debug!("timestamps = {timestamps}");

        info!(
            "Splitting '{}' into {} clips",
            metadata.title,
            timestamps.len()
        );

        clipper.create_clips(
            tmp.path(),
            &args.out,
            &timestamps,
            &metadata,
            &video_id,
            args.ext.with_dot(),
        );

        let nb_new_files = file_counter.count_new()?;
        let nb_expected = timestamps.len();
        if nb_new_files != nb_expected as isize {
            warn!(
                "Expected to have created {nb_expected} new clips, \
                    but {nb_new_files} new files have been found since last time"
            );
        }

        cache.push(video_id)?;
    }

    info!(
        "{} clips have been created during this session",
        file_counter.count() - initial_count
    );

    Ok(())
}

/// Load the external components
fn load_external_components(
    _args: &Args,
) -> Result<(impl StreamDownloader, impl StreamTransformer)> {
    // Construct the handles concurrently as executing an external program
    // is not instantaneous. That way we can avoid adding the costs
    let ytdl_thread = std::thread::spawn(Ytdl::new);
    let ffmpeg_thread = std::thread::spawn(Ffmpeg::new);

    let ytdl = ytdl_thread.join().expect("Could not join thread")?;
    let ffmpeg = ffmpeg_thread.join().expect("Could not join thread")?;

    Ok((ytdl, ffmpeg))
}

/// Load the main components. This will not write or download anything.
fn load_internal_components<'args, 'ext>(
    args: &'args Args,
    stream_tsf: &'ext impl StreamTransformer,
) -> Result<(AlreadyProcessed, FileCounter<'args>, Clipper<'ext>)> {
    let cache = AlreadyProcessed::read_or_create(&args.cache)
        .context("Could not create or read cache file")?;
    info!("{} items already processed", cache.len());

    let file_counter = FileCounter::new(&args.out).context("Could not initialize file counter")?;

    let clipper = Clipper::new(stream_tsf).context("Could not initialize the clipper")?;

    Ok((cache, file_counter, clipper))
}

fn download_and_extract_metadata<P: AsRef<Path>>(
    clipper: &Clipper,
    stream_dl: &impl StreamDownloader,
    video_id: &str,
    out: P,
    skip_timestamps: bool,
    clip_regex: &Regex,
) -> Result<(Metadata, Timestamps)> {
    let out = out.as_ref();

    let metadata = stream_dl.get_metadata(video_id)?;

    loop {
        info!("Downloading video {video_id}");
        stream_dl.download_audio(out, video_id)?;

        if skip_timestamps {
            info!("Downloaded file, skip timestamps extraction");

            let start = Timestamp {
                t_start: "00:00".to_string(),
                title: metadata.title.to_string(),
            };

            return Ok((metadata, Timestamps::new(vec![start])));
        } else {
            info!("Downloaded file, splitting from timestamps");

            let timestamps = clipper.extract_timestamps(&metadata.description, clip_regex)?;
            if clipper.is_file_complete(metadata.duration, &timestamps)? {
                return Ok((metadata, timestamps));
            } else {
                warn!("Downloaded file seems incomplete. Retry downloading it again");
            }
        }
    }
}

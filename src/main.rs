mod already_processed;
mod cli;
mod clipper;
mod command;
mod file_counter;
mod io;
mod types;

use std::path::Path;

use anyhow::{Context, Result};
use clap::Parser;
use cli::Split;
use log::{debug, error, info, warn};
use regex::Regex;
use types::{Metadata, Timestamps};

use crate::{
    already_processed::AlreadyProcessed,
    cli::Args,
    clipper::Clipper,
    command::{download_audio_with_meta, extract_metadata, get_playlist_videos_id},
    file_counter::FileCounter,
    io::named_tempfile,
    types::{Extension, Timestamp},
};

fn main() -> Result<()> {
    // Initialize the environment & CLI
    dotenv::dotenv().ok();
    env_logger::init();
    let args = Args::parse();

    // Make sure the output directory is created
    std::fs::create_dir_all(&args.out).context("Could not create output dir")?;

    let (mut cache, mut file_counter, clipper) = load_components(&args)?;

    // Download the playlist videos id
    info!("Get the playlist videos id");
    let videos_id = get_playlist_videos_id(&args.id).context("Could not get playlist videos id")?;
    info!("{} videos in the playlist", videos_id.len());

    // Put mkv as the stream file format as:
    // - Not giving any will cause an error (even though it may write another file format)
    // - It should accept any kind of audio format
    // With that, the stream data should be copied as-is, without modification
    let tmp = named_tempfile(Extension::Mkv)?;
    let initial_count = file_counter.count();
    let skip_timestamps = matches!(args.split, Split::Full);
    for video_id in videos_id {
        if cache.contains(&video_id) {
            info!("Video {video_id} already processed. Skipping it");
            continue;
        }

        if let Some((metadata, timestamps)) =
            download_and_extract_metadata(&video_id, tmp.path(), skip_timestamps, &args.clip_regex)
                .context("Could not download and extract metadata and timestamps")?
        {
            debug!("metadata = {metadata}");
            debug!("timestamps = {timestamps}");

            info!(
                "Splitting '{}' into {} clips",
                metadata.get("title").map_or("???", |s| s),
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
        }

        cache.push(video_id)?;
    }

    info!(
        "{} clips have been created during this session",
        file_counter.count() - initial_count
    );

    Ok(())
}

/// Load the main components. This will not write or download anything.
fn load_components(args: &Args) -> Result<(AlreadyProcessed, FileCounter, Clipper)> {
    let cache = AlreadyProcessed::read_or_create(&args.cache)
        .context("Could not create or read cache file")?;
    info!("{} items already processed", cache.len());

    let file_counter = FileCounter::new(&args.out).context("Could not initialize file counter")?;

    let clipper = Clipper::new().context("Could not initialize the clipper")?;

    Ok((cache, file_counter, clipper))
}

fn download_and_extract_metadata<P: AsRef<Path>>(
    video_id: &str,
    out: P,
    skip_timestamps: bool,
    clip_regex: &Regex,
) -> Result<Option<(Metadata, Timestamps)>> {
    let out = out.as_ref();

    loop {
        info!("Downloading video {video_id}");

        // Make sure the file does not exist before downloading or it may fail
        let _ = std::fs::remove_file(out);
        if !download_audio_with_meta(&out, video_id)? {
            error!(
                "Could not download video. Remove the video id from cache \
                and retry with RUST_LOG=debug for more information"
            );
            return Ok(None);
        }

        if skip_timestamps {
            info!("Downloaded file, skip timestamps extraction");

            let metadata = extract_metadata(out)
                .context("Could not extract description of downloaded file")?;

            let title = metadata.get("title").map_or("???", |s| s);
            let start = Timestamp {
                t_start: "00:00".to_string(),
                title: title.to_string(),
            };

            return Ok(Some((metadata, Timestamps::new(vec![start]))));
        } else {
            info!("Downloaded file, splitting from timestamps");

            let (metadata, timestamps) = Clipper::extract_metadata_timestamps(out, clip_regex)?;
            if Clipper::is_file_complete(out, &timestamps)? {
                return Ok(Some((metadata, timestamps)));
            } else {
                warn!("Downloaded file seems incomplete. Retry downloading it again");
            }
        }
    }
}

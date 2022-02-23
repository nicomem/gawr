mod already_processed;
mod cli;
mod clipper;
mod command;
mod file_counter;
mod io;
mod types;

use std::path::Path;

use anyhow::{bail, Context, Result};
use clap::Parser;
use cli::Split;
use log::{debug, error, info, warn};
use types::{Metadata, Timestamps};

use crate::{
    already_processed::AlreadyProcessed,
    cli::Args,
    clipper::Clipper,
    command::{download_audio_with_meta, extract_metadata, get_playlist_videos_id},
    file_counter::FileCounter,
    types::{Extension, Timestamp},
};

fn main() -> Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();

    let args = Args::parse();

    if Extension::from_path(&args.tmp).is_none() {
        bail!("Temporary file has incorrect extension");
    }

    std::fs::create_dir_all(&args.out).context("Could not create output dir")?;

    let mut cache = AlreadyProcessed::read_or_create(&args.cache)
        .context("Could not create or read cache file")?;
    info!("{} items already processed", cache.len());

    info!("Get the playlist videos id ...");

    let videos_id =
        get_playlist_videos_id(&args.playlist).context("Could not get playlist videos id")?;

    info!("{} videos in the playlist", videos_id.len());

    let mut file_counter =
        FileCounter::new(&args.out).context("Could not initialize file counter")?;
    let initial_count = file_counter.count();

    let clipper = Clipper::new().context("Could not initialize the clipper")?;
    let skip_timestamps = matches!(args.split, Split::Full);

    for video_id in videos_id {
        if cache.contains(&video_id) {
            info!("Video {video_id} already processed. Skipping it");
            continue;
        }

        if let Some((metadata, timestamps)) =
            download_and_extract_metadata(&video_id, &args.tmp, &mut cache, skip_timestamps)
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
                &args.tmp,
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
            std::fs::remove_file(&args.tmp)?;
        }
    }

    info!(
        "{} clips have been created during this session",
        file_counter.count() - initial_count
    );

    Ok(())
}

fn download_and_extract_metadata<P: AsRef<Path>>(
    video_id: &str,
    out: P,
    cache: &mut AlreadyProcessed,
    skip_timestamps: bool,
) -> Result<Option<(Metadata, Timestamps)>> {
    let out = out.as_ref();

    loop {
        info!("Downloading video {video_id}");
        if !download_audio_with_meta(&out, video_id)? {
            error!("Could not download video");
            cache.push(video_id.to_string())?;
            return Ok(None);
        }

        if skip_timestamps {
            let metadata = extract_metadata(out)
                .context("Could not extract description of downloaded file")?;

            info!("Downloaded file, skip timestamps extraction");

            let title = metadata.get("title").map_or("???", |s| s);
            let start = Timestamp {
                t_start: "00:00".to_string(),
                title: title.to_string(),
            };

            return Ok(Some((metadata, Timestamps::new(vec![start]))));
        }

        info!("Downloaded file, splitting from timestamps ...");

        let (metadata, timestamps) = Clipper::extract_metadata_timestamps(out)?;
        if Clipper::is_file_complete(out, &timestamps)? {
            return Ok(Some((metadata, timestamps)));
        } else {
            warn!("Downloaded file seems incomplete. Retry downloading it again");
        }
    }
}

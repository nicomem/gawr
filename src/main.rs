mod actors;
mod already_processed;
mod cli;
mod io;
mod my_regex;
mod outside;
mod result;
mod types;
mod utils;

use std::sync::{Arc, Mutex};

use actors::{
    connect_actors, Actor, ClipperActor, DownloadActor, TimestampActor, VideoId, VideoTitle,
};
use anyhow::Context;
use clap::Parser;
use cli::Split;
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use crossbeam_utils::thread::{scope, Scope};
use log::{debug, info};
use my_regex::DEFAULT_RE_LIST;
use outside::{Ffmpeg, StreamDownloader, StreamTransformer, Ytdl};

use crate::{already_processed::AlreadyProcessed, cli::Args, result::Result};

fn main() -> anyhow::Result<()> {
    // Initialize the environment & CLI
    dotenv::dotenv().ok();
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .init();
    let args = Args::parse();

    // Make sure the needed directories are created
    std::fs::create_dir_all(&args.out).context("Could not create out directory")?;
    if let Some(p) = args.cache.parent() {
        std::fs::create_dir_all(p).context("Could not create cache parent directories")?;
    }

    let (stream_dl, stream_tsf) = load_external_components(&args)?;
    let cache = AlreadyProcessed::read_or_create(&args.cache, &args.id)
        .context("Could not create or read cache file")?;
    info!("{} items already processed", cache.len());

    // Download the playlist videos id
    info!("Get the playlist videos id");
    let mut videos_id = stream_dl
        .get_playlist_videos_id(&args.id)
        .context("Could not get playlist videos id")?;
    info!("{} videos in the playlist", videos_id.len());

    if args.shuffle {
        debug!("Shuffling the playlist videos download order");
        fastrand::shuffle(&mut videos_id);
    }

    let cache = Arc::new(Mutex::new(cache));

    scope(|scope| -> Result<()> {
        let (input, output) = load_actors(scope, &stream_tsf, &stream_dl, &args, cache.clone())?;

        // Fill the input channel with all the tasks
        for video_id in videos_id {
            input.send(video_id).unwrap();
        }

        // Drop the input to indicate the end of the input data
        drop(input);

        // Wait for the output to be closed
        for _ in output {
            // Do nothing
        }

        Ok(())
    })
    .unwrap()?;

    info!("All tasks completed");
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

/// Link and load the actors in the scope and return the input and output channels
fn load_actors<'a>(
    scope: &Scope<'a>,
    stream_tsf: &'a dyn StreamTransformer,
    stream_dl: &'a dyn StreamDownloader,
    args: &'a Args,
    cache: Arc<Mutex<AlreadyProcessed>>,
) -> Result<(Sender<VideoId>, Receiver<VideoTitle>)> {
    // Run the clipper on all cpus except one to leave room for
    // the rest of the program to run
    let clipper_threads = std::thread::available_parallelism()?.get();

    let clip_regex = if let Some(clip_regex) = args.clip_regex.as_ref() {
        clip_regex
    } else {
        &DEFAULT_RE_LIST
    };

    let skip_timestamps = matches!(args.split, Split::Full);

    // Initialize the actors
    let mut dl_actor = DownloadActor::new(stream_dl, skip_timestamps, clip_regex, cache.clone());
    let mut tstamp_actor = TimestampActor::new();
    let mut clip_actors = Vec::with_capacity(clipper_threads);
    for id in 0..clipper_threads {
        clip_actors.push(ClipperActor::new(
            id,
            stream_tsf,
            &args.out,
            args.ext,
            cache.clone(),
        )?);
    }

    // Connect the actors together
    let (input, receive) = unbounded();
    dl_actor.set_receive_channel(receive);

    connect_actors(&mut dl_actor, &mut tstamp_actor, bounded(0));

    let (send, receive) = bounded(clipper_threads);
    for clip_actor in &mut clip_actors {
        connect_actors(
            &mut tstamp_actor,
            clip_actor,
            (send.clone(), receive.clone()),
        );
    }

    let (send, output) = unbounded();
    for clip_actor in &mut clip_actors {
        clip_actor.set_send_channel(send.clone());
    }

    // Start the actors
    scope.spawn(move |_| dl_actor.run().unwrap());
    scope.spawn(move |_| tstamp_actor.run().unwrap());
    for clip_actor in clip_actors {
        scope.spawn(move |_| clip_actor.run().unwrap());
    }

    Ok((input, output))
}

mod actors;
mod already_processed;
mod cli;
mod io;
mod my_regex;
mod outside;
mod result;
mod types;

use std::sync::{
    mpsc::{channel, sync_channel, Receiver, Sender},
    Arc, Mutex,
};

use actors::{Actor, ClipperActor, DownloadActor, VideoId, VideoTitle};
use anyhow::Context;
use clap::Parser;
use cli::Split;
use log::info;
use my_regex::DEFAULT_RE_LIST;
use outside::{Ffmpeg, StreamDownloader, StreamTransformer, Ytdl};
use rayon_core::Scope;

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
    let videos_id = stream_dl
        .get_playlist_videos_id(&args.id)
        .context("Could not get playlist videos id")?;
    info!("{} videos in the playlist", videos_id.len());

    let cache = Arc::new(Mutex::new(cache));

    rayon_core::scope(|scope| -> Result<()> {
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
    })?;

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
    let num_cpus = std::thread::available_parallelism()?;
    let clipper_threads = (num_cpus.get() - 1).min(1);

    let clip_regex = if let Some(clip_regex) = args.clip_regex.as_ref() {
        clip_regex
    } else {
        &DEFAULT_RE_LIST
    };

    let skip_timestamps = matches!(args.split, Split::Full);

    let (input, receive) = channel();
    let mut dl_actor = DownloadActor::new(stream_dl, skip_timestamps, clip_regex, cache.clone());
    dl_actor.set_receive_channel(receive);

    let mut clip_actor =
        ClipperActor::new(clipper_threads, stream_tsf, &args.out, args.ext, cache)?;
    let (send, receive) = sync_channel(0);
    clip_actor.set_receive_channel(receive);
    dl_actor.set_send_channel(send);

    let (send, output) = sync_channel(0);
    clip_actor.set_send_channel(send);

    scope.spawn(move |_| dl_actor.run().unwrap());
    scope.spawn(move |_| clip_actor.run().unwrap());

    Ok((input, output))
}

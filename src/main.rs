mod actors;
mod cli;
mod database;
mod io;
mod logging;
mod my_regex;
mod outside;
mod result;
mod types;
mod utils;

use std::num::NonZeroUsize;

use actors::{
    connect_actors, Actor, ClipperActor, DownloadActor, TimestampActor, VideoId, VideoTitle,
};
use clap::Parser;
use cli::Split;
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use miette::{Context, IntoDiagnostic};
use my_regex::DEFAULT_RE_LIST;
use outside::{Ffmpeg, StreamDownloader, StreamTransformer, Ytdl};
use tracing::{debug, info};

use crate::{
    cli::Args,
    database::{CacheDb, ProcessedState, Sqlite},
    logging::init_logging,
    result::Result,
};

fn main() -> miette::Result<()> {
    // Initialize the environment & CLI
    dotenv::dotenv().ok();

    let args = Args::parse();

    init_logging(args.log).wrap_err("Could not initialize logging")?;

    // Make sure the needed directories are created
    std::fs::create_dir_all(&args.out)
        .into_diagnostic()
        .wrap_err("Could not create out directory")?;
    if let Some(p) = args.cache.parent() {
        std::fs::create_dir_all(p)
            .into_diagnostic()
            .wrap_err("Could not create cache parent directories")?;
    }

    let (stream_dl, stream_tsf) = load_external_components(&args)
        .map_err(miette::Report::from)
        .wrap_err("Could not load external components")?;
    let cache = Sqlite::read_or_create(&args.cache).wrap_err("Could not load cache")?;
    let nb_videos = cache
        .count_videos(None)
        .wrap_err("Could not count videos in cache")?;
    let nb_completed = cache
        .count_videos(Some(ProcessedState::Completed))
        .wrap_err("Could not count videos in cache")?;
    let nb_pending = nb_videos - nb_completed;
    info!("{nb_videos} videos in cache: {nb_completed} completed and {nb_pending} pending");

    // Download the playlist videos id
    info!("Get the playlist videos id");
    let mut videos_id = stream_dl
        .get_playlist_videos_id(&args.id)
        .map_err(miette::Report::from)
        .wrap_err("Could not get playlist videos id")?;
    info!("{} videos in the playlist", videos_id.len());

    if args.shuffle {
        debug!("Shuffling the playlist videos download order");
        fastrand::shuffle(&mut videos_id);
    }

    std::thread::scope(|scope| -> Result<()> {
        let (input, output) = load_actors(scope, &stream_tsf, &stream_dl, &args, &cache)?;

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
    scope: &'a std::thread::Scope<'a, '_>,
    stream_tsf: &'a dyn StreamTransformer,
    stream_dl: &'a dyn StreamDownloader,
    args: &'a Args,
    cache: &'a Sqlite,
) -> Result<(Sender<VideoId>, Receiver<VideoTitle>)> {
    let nb_cores = NonZeroUsize::new(args.cores)
        .unwrap_or_else(|| std::thread::available_parallelism().unwrap());

    // Run the clipper on all cpus except one to leave room for
    // the rest of the program to run
    let clipper_threads = usize::max(1, nb_cores.get() - 1);

    let clip_regex = if let Some(clip_regex) = args.clip_regex.as_ref() {
        clip_regex
    } else {
        &DEFAULT_RE_LIST
    };

    let skip_timestamps = matches!(args.split, Split::Full);

    // Initialize the actors
    let mut dl_actor = DownloadActor::new(stream_dl, skip_timestamps, clip_regex, cache);
    let mut tstamp_actor = TimestampActor::new(cache);
    let mut clip_actors = Vec::with_capacity(clipper_threads);
    for id in 0..clipper_threads {
        clip_actors.push(ClipperActor::new(
            id,
            stream_tsf,
            &args.out,
            args.ext,
            cache,
            args.bitrate,
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
    std::thread::Builder::new()
        .name("DownloadActor".to_string())
        .spawn_scoped(scope, move || {
            dl_actor
                .run()
                .wrap_err("Download Actor crashed unexpectedly")
                .unwrap()
        })
        .into_diagnostic()?;

    std::thread::Builder::new()
        .name("TimestampActor".to_string())
        .spawn_scoped(scope, move || {
            tstamp_actor
                .run()
                .wrap_err("Timestamp Actor crashed unexpectedly")
                .unwrap()
        })
        .into_diagnostic()?;

    for (i, clip_actor) in clip_actors.into_iter().enumerate() {
        std::thread::Builder::new()
            .name(format!("ClipperActor-{i}"))
            .spawn_scoped(scope, move || {
                clip_actor
                    .run()
                    .wrap_err_with(|| format!("Clipper Actor {i} crashed unexpectedly"))
                    .unwrap()
            })
            .into_diagnostic()?;
    }

    Ok((input, output))
}

mod command;
mod io;
mod timestamp;

use std::{collections::HashMap, fmt::Display, fs::OpenOptions, io::Write, ops::Deref, path::Path};

use anyhow::{anyhow, Context, Result};
use io::build_output_path;
use log::{debug, error, info, warn};
use rayon::{ThreadPool, ThreadPoolBuilder};
use timestamp::{Timestamp, Timestamps};

use crate::{
    command::{
        download_audio_with_meta, extract_clip, extract_metadata, get_file_duration,
        get_playlist_videos_id, normalize_audio,
    },
    io::{load_already_processed, touch},
    timestamp::{extract_timestamps, timestamp_to_seconds},
};

#[derive(Debug)]
pub struct Metadata(HashMap<String, String>);

impl Display for Metadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{{")?;
        for (k, v) in self.iter() {
            writeln!(f, "\t{k}: {v}")?;
        }
        writeln!(f, "}}")?;
        Ok(())
    }
}

impl Deref for Metadata {
    type Target = HashMap<String, String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

const EXTENSION: &str = ".mkv";

fn main() -> Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();

    let env_var = |key: &str| {
        std::env::var(key).with_context(|| format!("Could not get env variable '{key}'"))
    };

    let playlist_id = env_var("PLAYLIST_ID")?;
    let processed_path = env_var("PROCESSED_PATH")?;
    let out_dir = env_var("OUT_DIR")?;
    let tmp_file = env_var("TMP_FILE")?;
    let debug_current_file = env_var("DEBUG_CURRENT_FILE").map_or(Ok(false), |s| {
        s.parse()
            .with_context(|| "Env variable 'DEBUG_CURRENT_FILE' could not be parsed into a bool")
    })?;

    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("Could not create dir: {out_dir}"))?;
    touch(&processed_path).with_context(|| format!("Could not touch {processed_path}"))?;

    let already_processed = load_already_processed(&processed_path)
        .with_context(|| "Could not load already processed file")?;

    info!("{} items already processed", already_processed.len());

    let pool = ThreadPoolBuilder::new().build()?;

    if debug_current_file {
        info!("Debugging current file. Do not download anything");

        let video_id = "DEBUG";

        if let Some((metadata, timestamps)) = extract_metadata_timestamps(&tmp_file)? {
            debug!("metadata = {metadata}");
            debug!("timestamps = {timestamps}");
            process_file(&tmp_file, &timestamps, &out_dir, metadata, video_id, &pool);
        }
    } else {
        info!("Get the playlist videos id ...");

        let count_out_files = || {
            Path::new(&out_dir)
                .read_dir()
                .with_context(|| "Could not read out_dir")
                .unwrap()
                .count()
        };

        let nb_previous_out_files = count_out_files();
        let mut nb_cur_out_files = nb_previous_out_files;

        let videos_id = get_playlist_videos_id(&playlist_id)
            .with_context(|| "Could not get playlist videos id")?;

        let mut processed_file = OpenOptions::new()
            .append(true)
            .open(&processed_path)
            .with_context(|| "Could not open processed file in append mode")?;

        for video_id in videos_id {
            if already_processed.contains(&video_id) {
                info!("Video {video_id} already processed. Skipping it");
                continue;
            }

            if let Some((metadata, timestamps)) =
                download_and_extract_metadata(&video_id, &tmp_file, &mut processed_file)
                    .with_context(|| "Could not download and extract metadata and timestamps")?
            {
                debug!("metadata = {metadata}");
                debug!("timestamps = {timestamps}");

                info!(
                    "Splitting '{}' into {} clips",
                    metadata.get("title").map_or("???", |s| s),
                    timestamps.len()
                );

                let clips_created =
                    process_file(&tmp_file, &timestamps, &out_dir, metadata, &video_id, &pool);

                let nb_new_out_files = count_out_files();
                let clips_created_real = nb_new_out_files - nb_cur_out_files;

                if clips_created_real != clips_created {
                    warn!("Expected to have created {clips_created} new clips, but {clips_created_real} new files have been found since last time");
                }
                nb_cur_out_files = nb_new_out_files;

                writeln!(processed_file, "{video_id}")
                    .with_context(|| "Could not append to processed file")?;

                std::fs::remove_file(&tmp_file)?;
            }
        }

        let nb_processed_files_real = nb_cur_out_files - nb_previous_out_files;
        info!("{nb_processed_files_real} clips have been created during this session");
    }

    Ok(())
}

fn download_and_extract_metadata<P1: AsRef<Path>>(
    video_id: &str,
    tmp_file: P1,
    mut processed_file: impl Write,
) -> Result<Option<(Metadata, Timestamps)>> {
    let tmp_file = tmp_file.as_ref();

    loop {
        info!("Downloading video {video_id}");
        if !download_audio_with_meta(&tmp_file, video_id)? {
            error!("Could not download video");
            writeln!(processed_file, "{video_id}")
                .with_context(|| "Could not append to processed file")?;
            return Ok(None);
        }

        info!("Downloaded file, splitting from timestamps ...");

        if let res @ Some(_) = extract_metadata_timestamps(tmp_file)? {
            return Ok(res);
        }
    }
}

fn extract_metadata_timestamps<P1: AsRef<Path>>(
    tmp_file: P1,
) -> Result<Option<(Metadata, Timestamps)>> {
    let metadata = extract_metadata(&tmp_file)
        .with_context(|| "Could not extract description of downloaded file")?;

    let description = &metadata["description"];
    let timestamps = extract_timestamps(description);

    if timestamps.len() < 5 {
        return Err(anyhow!(
        "Nope, too little timestamps, something went wrong. Description: {description}. Timestamps: {timestamps:?}"
    ));
    }

    let last_timestamp = timestamps.last().unwrap();
    let last_secs = timestamp_to_seconds(&last_timestamp.t_start);

    let file_duration =
        get_file_duration(&tmp_file).with_context(|| "Could not get file duration")?;

    if last_secs >= file_duration {
        // File is shorter than the last timestamp, retry downloading the file
        warn!("Downloaded file seems incomplete. Retry downloading it");
        std::fs::remove_file(&tmp_file)?;
        Ok(None)
    } else {
        Ok(Some((metadata, timestamps)))
    }
}

fn process_file<P1: AsRef<Path>, P2: AsRef<Path>>(
    tmp_file: P1,
    timestamps: &[Timestamp],
    out_dir: P2,
    metadata: Metadata,
    video_id: &str,
    pool: &ThreadPool,
) -> usize {
    let out_dir = out_dir.as_ref();
    let tmp_file = tmp_file.as_ref();
    let origin = format!(
        "{} ({})",
        metadata.get("title").map_or("???", |s| s),
        video_id
    );

    pool.scope(|s| {
        for sl in timestamps.windows(2) {
            if let [start, end] = sl {
                // Build the output file path and touch it in the main thread to avoid
                // problems if the same title appears multiple times in the same video
                let output = build_output_path(&out_dir, &start.title, EXTENSION)
                    .with_context(|| "Could not build output file path")
                    .unwrap();

                touch(&output)
                    .with_context(|| {
                        format!("Could not create the empty file {}", output.display())
                    })
                    .unwrap();

                s.spawn(|_| create_clip(tmp_file, output, start, Some(end), &origin));
            } else {
                unreachable!()
            }
        }

        // Do not forget to do the last timestamp up to the end of the stream
        if let Some(start) = timestamps.last() {
            let output = build_output_path(&out_dir, &start.title, EXTENSION)
                .with_context(|| "Could not build output file path")
                .unwrap();

            touch(&output)
                .with_context(|| format!("Could not create the empty file {}", output.display()))
                .unwrap();

            s.spawn(|_| create_clip(tmp_file, output, start, None, &origin));
        }
    });

    // Created this number of clips (or at least should have)
    timestamps.len()
}

fn create_clip<P1: AsRef<Path>, P2: AsRef<Path>>(
    tmp_file: P1,
    output: P2,
    start: &Timestamp,
    end: Option<&Timestamp>,
    origin: &str,
) {
    let tmp_file = tmp_file.as_ref();
    let output = output.as_ref();

    extract_clip(&tmp_file, &output, start, end, origin)
        .with_context(|| "Could not extract a clip of the audio file from the timestamps")
        .unwrap();

    normalize_audio(output)
        .with_context(|| "Could not normalize audio")
        .unwrap();

    debug!(
        "Clip number '{}' ({} - {}) completed",
        start.title,
        start.t_start,
        end.map_or("END", |end| end.t_start.as_str())
    );
}

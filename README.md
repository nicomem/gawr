# gawr

A music downloader archive tool thingie

## Pre-requisites

- A Rust environment to build the project
  - Tested on version 1.64
- Either `yt-dlp` or `youtube-dl`
- `ffmpeg`

## How to build & run

```bash
# To run locally
cargo build --release
cargo run --release -- --help

# To install & run from anywhere
cargo install --path .
cd /some/where/under/the/rainbow
gawr --help
```

To check the required / optional arguments, check the `--help` command, it should be quite descriptive.

The tool also reads environment variables and `.env` file, also check the `--help` command to see the environment variables to set, and their currently read values.
- This can be quite useful to avoid writing the same arguments every time, and simply run `gawr` to update the audio archive

## How it works

### Short version

- The tool downloads audio streams, potentially clip them and apply audio normalization before saving them
- All of this is done on multiple threads to minimize the time downloading / processing
- It saves the current state in a sqlite local database to be able to restart at any time without re-doing work it has already done

### Long version

#### Initialization

This is where the tool starts and:
- reads the `.env` file
- parses the command line arguments
- checks if the external programs are present (`yt-dlp` or `youtube-dl`, and `ffmpeg`)
- initializes the [actors](#the-actors)

### The actors

The main processing loop is structured using an [actor model](https://en.wikipedia.org/wiki/Actor_model) as schematized below :

![](./docs/actor-model.svg)

1. The list of playlist video IDs is downloaded, and compared to the sqlite cache database to see whether there are new video stream to download

2. Each of these video IDs is sent to the *Download Actor* which downloads the video audio stream and passes them to the next actor

3. The *Timestamp Actor* parses the video description to detect timestamps in the video. It then passes each video section (a start time, and an end time) to one of the next actors

4. The *Clipper Actors* clips the audio file to keep only the wanted video section, applies audio normalization and other `ffmpeg` conversions to get the output clip audio file

The actor model is useful here since it allows each actor to run on its own thread and thus to optimize the work done conurrently :

- As soon as the *Download Actor* has passed the audio file to the next actor, it will begin downloading the next one.
- Timestamps for different audio files can be processed at the same time

### When things go bad

As this tool uses external programs, which communicate through the network, potentially using non-standard APIs... errors are bound to happen.

A lot has been done to handle as best as possible failures:

- Retry operations
- Detect unavailable video streams
- Process files using temporary files, to avoid trashing the output directory in case of crash/failure
- Save current state to handle unexpected crashes of the tool

At this point, the tool can be expected to work decently for personal usage, and should not require manual fiddling to put it out of a trash state. (but if that happens, feel free to create a new issue)

## Contributing

Feel free to create issues and/or submit pull request. I cannot guarantee how long it would take to solve/review them as it depends on how busy I am at the moment.

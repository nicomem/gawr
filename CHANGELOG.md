# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.5] - 2024-03-28
### Fixed
- Fix nix packaging by using `crane` instead of `dream2nix`

## [1.0.4] - 2023-06-11
### Added
- Package project for nix using `dream2nix`

### Changed
- Replace usage of `once_cell` crate with the standard library `OnceLock`

## [1.0.3] - 2023-02-04
### Added
- Added automatic Github release building using `cargo-dist`

## [1.0.2] - 2022-11-05
### Changed
- On Unix systems: Try to set output file permission to 644 (read-write for user, read-only for the rest)
    - If this fails, the permissions set on the file creation are left
    - The new permissions are the common ones for created files and avoids problems with third-party programs trying to read the files as another user of the system

## [1.0.1] - 2022-10-02
### Changed
- Clap upgraded to version 4, this changes how the CLI is displayed

## [1.0.0] - 2022-08-14
### Added
- Now reads from a TOML configuration file
- More problematic characters are removed from output filenames
- Add project CI

### Changed
- Rework the CLI to allow for reading configuration from a TOML file
    - This allows for more flexible configuration
    - But is more complex and do not have all the nice `clap` things right out of the box

## [0.9.0] - 2022-07-29
### Added
- Add the `log` and `bitrate` command line argument
- Name threads based on the actor (+ number if multiple actors of the same type)
    - Logs are prettier as they use the thread name instead of the rust module name
- A beautiful *chef's kiss* README

### Changed
- Change the logging system from `log` to `tracing`
    - Logs should be more compact

### Fixed
- If `ffmpeg` is not installed, the error message simply says so instead of cryptic things

## [0.8.0] - 2022-03-27
### Added
- Add option to assume a certain number of cores instead of automatically using the system's logical cores
- Now able to properly resume after a crash, remembering which clips have been or not processed
- Delete every empty file in the output directory at startup
    - This avoids creating clips with incremented indexes after a crash

### Changed
- Switch to [miette](https://github.com/zkat/miette) for prettier error reporting
- Use a Sqlite database for cache instead of a custom text format
    - This enables more flexibility for future features at the cost of a worse readable file
    - **The old format is not compatible anymore and cache must be regenerated**

## [0.7.0] - 2022-03-13
### Added
- Refactor `ClipperActor` to handle more clips in parallel
    - Before it processed all clips of one stream in its own thread pool
    - Now multiple `ClipperActor` are spawned and can process clips from multiple streams in parallel
    - This means that:
        - Multiple streams will be processed in parallel in `--split=full` mode
        - CPU will not be underused during the last clips in `--split=clips` mode
    - tldr: **things will go faster**
- Add `TimestampActor` whose role is only to dispatch the timestamped clips produced by the `DownloadActor`
    - This enables the previous actor to directly begin downloading the next stream instead of waiting that the next actor receives the channel message
    - tldr: **less waiting between stream downloads**

### Fixed
- Fix incorrect number of threads used for clipping

## [0.6.1] - 2022-03-13
### Added
- Add crate information from `Cargo.toml` to binary cli

### Fixed
- Fix a mutex deadlock

## [0.6.0] - 2022-03-03
### Added
- Empty output placeholders now have the `.empty` extension
    - So if something bad happens, it is easier to see what is right and what is wrong
- Add an option to randomize the order in which videos are downloaded
- Allow cache comments after the content in a line
    - But only consider comment-only lines as section titles
- Added compilation flags in release mode for reducing the binary size (6.5M -> 2M)

## [0.5.0] - 2022-03-03
### Added
- Cache comments
    - Allow adding comments in the cache file, which will be ignored by it
    - Ignore blank lines in the cache
    - Add comments in the cache with the playlist ID used
- Refactor the main pipeline to use the Actor design pattern
    - Each actor runs concurrently and exchange messages through a one-way channel
    - 2 actors: one for downloading, the other for processing
    - This allows for downloading and processing at the same time
    - Uses rendez-vous channels to avoid high disk/memory usage
- Test multiple patterns for timestamp detection
    - For every line in the description, every regex will be tested until one matches
    - Update the default pattern(s) to detect more timestamps

### Changed
- Remove more problematic characters from title (for their file names)
- Do all processes to temporary files, then simply move/copy to the output path
    - Meaning that at all times, an output file is either empty or complete
- Moved from `convert_case` dependency to `heck`
    - The former did not handle some unicode titles correctly
- Move very long debug logs to trace and add more logs

### Removed
- Do not verify the number of files created at each iteration

## [0.4.0] - 2022-02-27
### Added
- Verify that the external programs are reachable at startup
- If `yt-dlp` is not present, try reaching `youtube-dl`
- Add custom error enumeration
- Differentiate more precisely between unavailable videos and other unexpected errors

### Changed
- Refactor the commands into multiple interfaces to external components

### Removed
- Remove the dependency on `ffprobe`

## [0.3.0] - 2022-02-26
### Added
- Create temporary files automatically
- Support more file format: `mka` and `webm`
- Allow using a different regular expression for finding and parsing description timestamps

### Changed
- Change audio normalization backend from `ffmpeg-normalize` to calls to `ffmpeg`
- Do not use the user-given extension for the entire stream temporary file
    - Use directly `mkv` as it should support nearly anything
- Set the default file extension to `ogg`

### Removed
- Remove the dependency on `ffmpeg-normalize`
- Remove the option to select a temporary file path

## [0.2.0] - 2022-02-25
### Added
- Add a command line argument parser
- Output file extension can be configured
- Allow to either split the file into clips or keep the entire file

### Changed
- Environment variables are prefixed by the uppercased project name
- Structure code into more, smaller files & components
- Verify that the temporary file has a verified valid extension at startup
    - Instead of erroring out during command execution

## [0.1.0] - 2022-02-13
### Added
- First complete version of the binary
- Download the audio of every video in a playlist
- Split each audio file into clips based on the video description
- Normalize audio clips with ffmpeg-normalize
- Read environment variables to alter the runtime execution
- Load .env files into the environment variables

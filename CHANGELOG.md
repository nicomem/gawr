# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

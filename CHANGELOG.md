# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2022-02-25
### Added
- Add a command line argument parser
- Output file extension can be configured
- Allow to either split the file into clips or keep the entire file

### Changed
- Environment variables are prefixed by the uppercased project name
- Structure code into more, smaller files & components
- Verify that the temporary file has a verified valid extension at startup instead of erroring out during command execution

## [0.1.0] - 2022-02-13
### Added
- First complete version of the binary
- Download the audio of every video in a playlist
- Split each audio file into clips based on the video description
- Normalize audio clips with ffmpeg-normalize
- Read environment variables to alter the runtime execution
- Load .env files into the environment variables

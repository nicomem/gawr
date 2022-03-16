use std::sync::Arc;

use tempfile::NamedTempFile;

use crate::{
    database,
    types::{Metadata, Timestamp, Timestamps},
};

pub type VideoId = String;
pub type VideoTitle = String;

#[derive(Debug)]
pub struct DownloadedStream {
    pub video_id: String,
    pub file: NamedTempFile,
    pub metadata: Metadata,
    pub timestamps: Timestamps,
    pub db_id: database::VideoId,
    pub video_state: database::ProcessedState,
}

pub struct StreamInfo {
    pub video_id: String,
    pub stream_file: NamedTempFile,
    pub metadata: Metadata,
    pub db_id: database::VideoId,
}

pub struct TimestampedClip {
    pub stream_info: Arc<StreamInfo>,
    pub start: Timestamp,
    pub end: Option<Timestamp>,
    pub clip_idx: database::ClipIdx,
}

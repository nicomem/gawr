use std::sync::Arc;

use tempfile::NamedTempFile;

use crate::types::{Metadata, Timestamp, Timestamps};

pub type VideoId = String;
pub type VideoTitle = String;

#[derive(Debug)]
pub struct DownloadedStream {
    pub video_id: String,
    pub file: NamedTempFile,
    pub metadata: Metadata,
    pub timestamps: Timestamps,
}

pub struct StreamInfo {
    pub video_id: String,
    pub stream_file: NamedTempFile,
    pub metadata: Metadata,
}

pub struct TimestampedClip {
    pub stream_info: Arc<StreamInfo>,
    pub start: Timestamp,
    pub end: Option<Timestamp>,
}

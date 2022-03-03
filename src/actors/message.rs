use tempfile::NamedTempFile;

use crate::types::{Metadata, Timestamps};

pub type VideoId = String;
pub type VideoTitle = String;

#[derive(Debug)]
pub struct DownloadedStream {
    pub video_id: String,
    pub file: NamedTempFile,
    pub metadata: Metadata,
    pub timestamps: Timestamps,
}

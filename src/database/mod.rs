mod sqlite;

use std::path::Path;

use miette::Result;

pub use sqlite::Sqlite;

pub type ClipIdx = u16;

/// Identifier that can be used to refer to video instead of using the
/// video_id string.
///
/// This identifier **must** be unique for each video in the database, and
/// **must** remain that way unless explicitely stated so.
pub type VideoId = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessedState {
    /// The video has not been processed
    NotProcessed,

    /// The video has been partially processed.
    /// This contains the list of clip indexes that **remains to be processed**.
    RemainingClips(Vec<ClipIdx>),

    /// The video has been partially processed.
    /// This contains the list of clip indexes that **have been processed**.
    #[allow(dead_code)] // not used currently but may be useful in the future
    ProcessedClips(Vec<ClipIdx>),

    /// The video has been entirely processed
    Completed,
}

/// A trait for saving useful application data between multiple executions.
///
/// This can be used to avoid repeating already done computation or being
/// able to continue work after an unexpected crash.
pub trait CacheDb
where
    Self: Sized + Sync,
{
    /// Read the cache file at the given path or create it if it does not exist.
    ///
    /// If the file does exist but does not correspond to a valid database file,
    /// an error **should** be returned.
    fn read_or_create(p: &Path) -> Result<Self>;

    /// Check the state of the video, whether it have been completed or needs
    /// more processing.
    /// Also return the database preferred video ID value.
    fn check_video(&self, video_id: &str) -> Result<(VideoId, ProcessedState)>;

    /// Inform the database that the video needs this number of
    /// clips to be fully processed.
    ///
    /// If the video had previously been assigned work, this **should** overwrite it
    /// along with the previous progress made.
    fn assign_work(&self, video: VideoId, nb_clips: ClipIdx) -> Result<()>;

    /// Inform the database that the clip with the specified index
    /// has been processed.
    ///
    /// The indexes are zero-based (0 to len-1).
    /// The indexes may not be completed in order.
    ///
    /// Once all units of work have been completed, the database **may**
    /// internally mark the video as fully completed or wait for a call to [set_video_as_completed].
    fn complete_work(&self, video: VideoId, clip_idx: ClipIdx) -> Result<()>;

    /// Inform the database that all the work for the video has been done.
    ///
    /// This **should** be called after all work has been informed
    /// to be completed to the database.
    fn set_video_as_completed(&self, video: VideoId) -> Result<()>;

    /// Count the number of videos in the database.
    ///
    /// If a filter is specified, only count those that are in the given state.
    fn count_videos(&self, filter: Option<ProcessedState>) -> Result<usize>;
}

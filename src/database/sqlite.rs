use std::{fmt::Write, path::Path, sync::RwLock};

use log::debug;
use miette::{Context, IntoDiagnostic, Result};
use rusqlite::{
    params,
    types::{FromSql, FromSqlError, FromSqlResult, ToSqlOutput, Value, ValueRef},
    Connection, OptionalExtension, ToSql,
};

use super::{CacheDb, ClipIdx, ProcessedState, VideoId};

#[derive(Debug)]
pub struct Sqlite {
    conn: RwLock<Connection>,
}

unsafe impl Sync for Sqlite {}

impl CacheDb for Sqlite {
    fn read_or_create(p: &Path) -> Result<Self> {
        let cache = Self {
            conn: RwLock::new(
                Connection::open(p)
                    .into_diagnostic()
                    .wrap_err("Could not open sqlite file")?,
            ),
        };

        cache.create_tables().wrap_err("Could not create tables")?;

        Ok(cache)
    }

    fn check_video(&self, video_id: &str) -> Result<(VideoId, ProcessedState)> {
        let conn = self.conn.read().unwrap();

        // Try to get the corresponding row
        if let Some((id, status, work_len)) = conn
            .query_row(
                "SELECT id, status, work_len FROM videos WHERE str_id = ?",
                [video_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .into_diagnostic()
            .wrap_err("Could not query specified video row")?
        {
            // Define query types
            let status: SqliteProcessedState = status;
            let work_len: Option<u32> = work_len;

            let status: ProcessedState = status.0;

            // Simple case: no need to check more of the database
            if status == ProcessedState::Completed || work_len.is_none() {
                return Ok((id, status));
            }

            // Harder case: check the work to do
            let conn = self.conn.read().unwrap();
            let mut stmt = conn
                .prepare(
                    "SELECT clip_idx FROM work
                    WHERE video_id = ?",
                )
                .into_diagnostic()?;

            let work_indexes = stmt
                .query_map([id], |row| row.get(0))
                .into_diagnostic()
                .wrap_err("Could not query corresponding work rows")?
                .flatten()
                .collect();
            Ok((id, ProcessedState::RemainingClips(work_indexes)))
        } else {
            drop(conn);
            let conn = self.conn.write().unwrap();

            // Video not in the table, insert it and get back the id
            debug!("Video not in the table, inserting it");
            let start_state = ProcessedState::NotProcessed;
            let id = conn
                .query_row(
                    "INSERT INTO videos (status, str_id)
                    VALUES (?, ?)
                    RETURNING id",
                    params![SqliteProcessedState(start_state.clone()), video_id],
                    |row| row.get(0),
                )
                .into_diagnostic()
                .wrap_err("Could not insert new video row")?;

            Ok((id, start_state))
        }
    }

    fn assign_work(&self, video: VideoId, nb_clips: ClipIdx) -> Result<()> {
        let conn = self.conn.write().unwrap();

        // Delete any previous work
        debug!("Deleting all old work of video {video}");
        conn.execute("DELETE FROM work WHERE video_id = ?", [video])
            .into_diagnostic()
            .wrap_err("Could not delete previous work rows")?;

        // Add every new work
        debug!("Assigning new work of length {nb_clips} for video {video}");
        let mut query = String::from("INSERT INTO work (video_id, clip_idx) VALUES\n");
        for idx in 0..nb_clips {
            writeln!(query, "({video}, {idx}),").unwrap();
        }
        query.pop(); // Remove newline
        query.pop(); // Remove comma
        conn.execute(&query, [])
            .into_diagnostic()
            .wrap_err("Could not insert new assigned work rows")?;

        // Set the work length to the video
        conn.execute(
            "UPDATE videos
            SET work_len = ?
            WHERE id = ?",
            params![nb_clips, video],
        )
        .into_diagnostic()
        .wrap_err("Could not update video with new work length")?;

        Ok(())
    }

    fn complete_work(&self, video: VideoId, clip_idx: ClipIdx) -> Result<()> {
        let conn = self.conn.write().unwrap();

        debug!("Complete work {clip_idx} of video {video}");
        conn.execute(
            "DELETE FROM work WHERE video_id = ? AND clip_idx = ?",
            params![video, clip_idx],
        )
        .into_diagnostic()?;
        Ok(())
    }

    fn set_video_as_completed(&self, video: VideoId) -> Result<()> {
        let conn = self.conn.write().unwrap();

        // Set as completed
        debug!("Set video {video} as completed");
        conn.execute(
            "UPDATE videos
            SET status = ?
            WHERE id = ?",
            params![SqliteProcessedState(ProcessedState::Completed), video],
        )
        .into_diagnostic()
        .wrap_err("Could not set video as completed")?;

        // Delete any potential remaining work
        debug!("Deleting all work of video {video}");
        conn.execute("DELETE FROM work WHERE video_id = ?", [video])
            .into_diagnostic()
            .wrap_err("Could not delete previous remaining work")?;

        Ok(())
    }

    fn count_videos(&self, filter: Option<ProcessedState>) -> Result<usize> {
        let conn = self.conn.read().unwrap();

        Ok(if let Some(filter) = filter {
            conn.query_row(
                "SELECT COUNT(id) FROM videos WHERE status = ?",
                [SqliteProcessedState(filter)],
                |row| row.get(0),
            )
            .into_diagnostic()?
        } else {
            conn.query_row("SELECT COUNT(id) FROM videos", [], |row| row.get(0))
                .into_diagnostic()?
        })
    }
}

impl Sqlite {
    /// Create the tables if they do not already exist
    fn create_tables(&self) -> Result<()> {
        let conn = self.conn.write().unwrap();

        conn.execute_batch(
            "BEGIN;
            CREATE TABLE IF NOT EXISTS videos (
                id          INTEGER PRIMARY KEY,
                status      INTEGER,
                str_id      TEXT NOT NULL,
                work_len    INTEGER
            );
            CREATE TABLE IF NOT EXISTS work (
                video_id    INTEGER,
                clip_idx    INTEGER,

                PRIMARY KEY (video_id, clip_idx),

                FOREIGN KEY (video_id)
                    REFERENCES videos (id)
                    ON DELETE CASCADE
                    ON UPDATE NO ACTION
            );
            COMMIT;",
        )
        .into_diagnostic()?;
        Ok(())
    }
}

/// Wrapper around [ProcessedState] so that it can be read from/written to sqlite
#[derive(Debug)]
struct SqliteProcessedState(ProcessedState);

impl FromSql for SqliteProcessedState {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let state = match value.as_i64()? {
            // We store whether the video is fully completed,
            // for other states, we need to check other parts of the database
            0 => ProcessedState::NotProcessed,
            1 => ProcessedState::Completed,
            n => return Err(FromSqlError::OutOfRange(n)),
        };

        Ok(SqliteProcessedState(state))
    }
}

impl ToSql for SqliteProcessedState {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        let val = match self.0 {
            // Simple cases
            ProcessedState::NotProcessed => 0,
            ProcessedState::Completed => 1,

            // These are not fully completed so 0
            ProcessedState::RemainingClips(_) => 0,
            ProcessedState::ProcessedClips(_) => 0,
        };

        Ok(ToSqlOutput::Owned(Value::Integer(val)))
    }
}

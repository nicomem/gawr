use std::path::Path;

use anyhow::{Context, Result};

pub struct FileCounter<'a> {
    directory: &'a Path,
    count: usize,
}

impl<'a> FileCounter<'a> {
    pub fn new(directory: &'a Path) -> Result<Self> {
        let count = Self::count_files(directory)?;
        Ok(Self { directory, count })
    }

    /// Count the number of files in the directory
    fn count_files(directory: &Path) -> Result<usize> {
        Ok(directory
            .read_dir()
            .context("Could not read directory")?
            .count())
    }

    /// Count and update the internal state.
    /// Return the number of new files.
    /// If the returned number is negative, there are less files than before.
    pub fn count_new(&mut self) -> Result<isize> {
        let old_count = self.count;
        self.count = Self::count_files(self.directory)?;

        Ok(self.count as isize - old_count as isize)
    }

    /// Return the number of files that have been counted during the last update
    pub fn count(&self) -> usize {
        self.count
    }
}

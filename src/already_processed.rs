use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::Path,
};

use crate::result::{bail, Result};

pub struct AlreadyProcessed {
    ids: BTreeSet<String>,
    file: File,
}

impl AlreadyProcessed {
    const COMMENT_PREFIX: &'static str = "# ";

    pub fn read_or_create<P: AsRef<Path>>(path: P, section_title: &str) -> Result<Self> {
        // Open or create file from the start with RW rights
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path.as_ref())?;

        // Read the entire file with each line as an id
        let reader = BufReader::new(&mut file);

        let mut is_same_section = false;
        let ids = reader
            .lines()
            .flatten()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty()) // Ignore blank lines
            .filter(|line| {
                // Ignore comments
                if let Some(comment) = Self::parse_comment(line) {
                    is_same_section = comment == section_title;
                    false
                } else {
                    true
                }
            })
            .collect();

        // If the last section is not the same as the new one,
        // insert the new section title as a comment
        if !is_same_section {
            writeln!(file, "{}", Self::to_comment(section_title))?;
        }

        Ok(Self { ids, file })
    }

    pub fn push(&mut self, id: String) -> Result<()> {
        // Push it to memory
        let is_new = self.ids.insert(id.clone());
        if !is_new {
            bail("ID already exist in cache")?;
        }

        // Push it to file
        writeln!(self.file, "{id}")?;
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.ids.contains(id)
    }

    fn to_comment(msg: &str) -> String {
        format!("{}{msg}", Self::COMMENT_PREFIX)
    }

    fn parse_comment(line: &str) -> Option<&str> {
        line.strip_prefix(Self::COMMENT_PREFIX)
    }
}

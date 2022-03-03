use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::Path,
};

use crate::result::{bail, Result};

#[derive(Debug)]
pub struct AlreadyProcessed {
    ids: BTreeSet<String>,
    file: File,
}

impl AlreadyProcessed {
    const COMMENT_PREFIX: &'static str = "# ";
    const COMMENT_DELIMITER: char = '#';

    pub fn read_or_create(path: &Path, section_title: &str) -> Result<Self> {
        // Open or create file from the start with RW rights
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;

        // Read the entire file with each line as an id
        let reader = BufReader::new(&mut file);

        let mut is_same_section = false;
        let ids = reader
            .lines()
            .flatten()
            .map(|line| line.trim().to_string())
            .map(|line| {
                if let Some((content, comment)) = line.split_once(Self::COMMENT_DELIMITER) {
                    // If the line is only comment, it is a section title
                    is_same_section = content.is_empty() && comment == section_title;
                    content
                } else {
                    &line
                }
                .trim()
                .to_string()
            })
            .filter(|line| !line.is_empty()) // Ignore blank lines
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
}

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
    pub fn read_or_create<P: AsRef<Path>>(path: P) -> Result<Self> {
        // Open or create file from the start with RW rights
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path.as_ref())?;

        // Read the entire file with each line as an id
        let reader = BufReader::new(&mut file);

        let ids = reader
            .lines()
            .flatten()
            .map(|s| s.trim().to_string())
            .collect();

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
}

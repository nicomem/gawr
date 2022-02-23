use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::Path,
};

use anyhow::{ensure, Context, Result};

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
            .open(path.as_ref())
            .context("Could not open processed file in append mode")?;

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
        ensure!(is_new, "ID already exist in cache");

        // Push it to file
        writeln!(self.file, "{id}").context("Could not append to processed file")?;
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.ids.contains(id)
    }
}

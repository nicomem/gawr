use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

pub fn touch<P: AsRef<Path>>(path: P) -> Result<()> {
    OpenOptions::new().create(true).append(true).open(path)?;
    Ok(())
}

pub fn load_already_processed<P: AsRef<Path>>(path: P) -> Result<BTreeSet<String>> {
    let f = File::open(path).with_context(|| "Could not open file")?;
    let reader = BufReader::new(f);

    Ok(reader
        .lines()
        .flatten()
        .map(|s| s.trim().to_string())
        .collect())
}

pub fn build_output_path<P: AsRef<Path>>(
    out_dir: P,
    title: &str,
    extension: &str,
) -> Result<PathBuf> {
    let out_dir = out_dir.as_ref();

    let mut output = out_dir.to_path_buf();
    let mut check_filename = |filename: &str| {
        output.extend(std::iter::once(filename));
        if !output.exists() {
            return Some(output.clone());
        }
        output.pop();
        None
    };

    // Check filenames one by one until one does not exist

    // Format for 1st file: <title><ext>
    if let Some(output) = check_filename(&format!("{title}{extension}")) {
        return Ok(output);
    }

    // Format for 2nd file and up: <title> (<count>)<ext>
    for n in 2u16.. {
        if let Some(output) = check_filename(&format!("{title} ({n}){extension}")) {
            return Ok(output);
        }
    }

    Err(anyhow::anyhow!(
        "Code is broken or you have really REALLY too much files with the same title"
    ))
}

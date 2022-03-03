use std::{
    fs::OpenOptions,
    path::{Path, PathBuf},
};

use tempfile::NamedTempFile;

use crate::{
    result::{bail, Result},
    types::Extension,
};

pub fn touch(path: &Path) -> Result<()> {
    OpenOptions::new().create(true).append(true).open(path)?;
    Ok(())
}

pub fn find_unused_prefix(
    out_dir: &Path,
    title: &str,
    extension: Extension,
    check_empty: bool,
) -> Result<PathBuf> {
    let mut output = out_dir.to_path_buf();

    let test_output = |output: &Path| {
        !(output.exists() || (check_empty && output.with_extension("empty").exists()))
    };

    let dot_ext = extension.with_dot();

    // Check filenames one by one until one does not exist

    // Format for 1st file: <title><ext>
    output.push(format!("{title}{dot_ext}"));
    if test_output(&output) {
        return Ok(output);
    }

    // Format for 2nd file and up: <title> (<count>)<ext>
    for n in 2u16.. {
        output.set_file_name(format!("{title} ({n}){dot_ext}"));
        if test_output(&output) {
            return Ok(output);
        }
    }

    bail("Code is broken or you have really REALLY too much files with the same title")
}

/// Create a named temporary file and return its handle.
///
/// The file destructor will be called at the handle drop.
/// **As such, one must not simply get the file path and drop the handle.**
pub fn named_tempfile(extension: Extension) -> Result<NamedTempFile> {
    Ok(tempfile::Builder::new()
        .suffix(extension.with_dot())
        .tempfile()?)
}

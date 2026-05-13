use super::archive::read_archive_from_zip;
use super::model::Sb3Archive;
use anyhow::{Context, Result};
use std::fs;
use std::io::Cursor;
use std::path::Path;
use zip::ZipArchive;

pub fn read_sb3_file(path: &Path) -> Result<Sb3Archive> {
    let bytes = fs::read(path).with_context(|| format!("Failed to read '{}'.", path.display()))?;
    read_sb3_bytes_with_label(&bytes, &path.display().to_string())
}

pub fn read_sb3_bytes(bytes: &[u8]) -> Result<Sb3Archive> {
    read_sb3_bytes_with_label(bytes, "memory")
}

fn read_sb3_bytes_with_label(bytes: &[u8], label: &str) -> Result<Sb3Archive> {
    let mut zip = ZipArchive::new(Cursor::new(bytes))
        .with_context(|| format!("'{}' is not a valid zip/.sb3 file.", label))?;
    let (project, assets) = read_archive_from_zip(&mut zip, label)?;
    Ok(Sb3Archive::new(project, assets))
}

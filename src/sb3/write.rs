use super::archive::write_archive_to_zip;
use super::model::Sb3Archive;
use anyhow::Result;
use std::fs;
use std::io::Cursor;
use std::path::Path;

pub fn write_sb3_file(path: &Path, archive: &Sb3Archive) -> Result<()> {
    let bytes = build_sb3_bytes(archive)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)?;
    Ok(())
}

pub fn build_sb3_bytes(archive: &Sb3Archive) -> Result<Vec<u8>> {
    let mut out = Cursor::new(Vec::<u8>::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    write_archive_to_zip(&mut zip, &archive.project, &archive.assets)?;
    zip.finish()?;
    Ok(out.into_inner())
}

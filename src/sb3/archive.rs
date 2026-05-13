use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{Read, Seek, Write};
use zip::write::SimpleFileOptions;
use zip::ZipArchive;

pub(crate) fn read_archive_from_zip<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    source_label: &str,
) -> Result<(Value, BTreeMap<String, Vec<u8>>)> {
    let mut project_json_str = String::new();
    {
        let mut entry = zip
            .by_name("project.json")
            .map_err(|_| anyhow!("project.json not found in '{}'.", source_label))?;
        entry
            .read_to_string(&mut project_json_str)
            .with_context(|| format!("Failed reading project.json in '{}'.", source_label))?;
    }
    let project = serde_json::from_str(&project_json_str)
        .with_context(|| format!("Invalid project.json inside '{}'.", source_label))?;

    let mut assets = BTreeMap::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let name = entry.name().to_string();
        if name == "project.json" || name.ends_with('/') {
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        assets.insert(name, bytes);
    }

    Ok((project, assets))
}

pub(crate) fn write_archive_to_zip<W: Write + Seek>(
    zip: &mut zip::ZipWriter<W>,
    project: &Value,
    assets: &BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("project.json", opts)?;
    zip.write_all(&serde_json::to_vec_pretty(project)?)?;

    for (name, bytes) in assets {
        zip.start_file(name, opts)?;
        zip.write_all(bytes)?;
    }

    Ok(())
}

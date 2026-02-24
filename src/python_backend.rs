use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;
use tempfile::NamedTempFile;

pub fn compile_with_python(input_path: &Path, merged_source: &str, output_path: &Path, no_svg_scale: bool) -> Result<()> {
    let temp_dir = input_path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp = NamedTempFile::new_in(temp_dir)
        .context("Failed to create temporary merged source file in input directory.")?;
    std::io::Write::write_all(&mut temp, merged_source.as_bytes())?;

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("Failed to locate repository root from rust_compiler/Cargo.toml path.")?;
    let compiler_py = repo_root.join("compiler.py");
    if !compiler_py.exists() {
        bail!("Python backend script not found: '{}'.", compiler_py.display());
    }

    let mut cmd = Command::new("python");
    cmd.current_dir(repo_root);
    cmd.arg(&compiler_py).arg(temp.path()).arg(output_path);
    if no_svg_scale {
        cmd.arg("--no-svg-scale");
    }

    let output = cmd.output().context(
        "Failed to start Python backend. Ensure `python` is available or remove --python-backend.",
    )?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        bail!(
            "Python backend failed while compiling '{}' -> '{}'\n{}\n{}",
            input_path.display(),
            output_path.display(),
            stdout.trim(),
            stderr.trim()
        );
    }
    Ok(())
}

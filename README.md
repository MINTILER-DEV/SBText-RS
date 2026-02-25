# sbtext-rs

Native Rust entrypoint for SBText with import resolution/validation implemented in Rust.

Current state:

- Resolves `import [SpriteName] from "path.sbtext"` recursively.
- Enforces top-level-only imports.
- Detects circular imports.
- Enforces imported-file sprite constraints and final duplicate sprite-name constraints.
- Can emit merged source via `--emit-merged`.
- Uses native Rust backend for `.sb3` generation by default.
- Supports Pen extension blocks and auto-adds `"pen"` to `project.json` when used.
- Keeps native CLI support and now also exposes a reusable Rust library API.

## Language docs

- Full syntax and semantics reference: `SYNTAX.md`

## Build

```bash
cargo build --release
```

## Usage

```bash
sbtext-rs INPUT OUTPUT
sbtext-rs INPUT OUTPUT --no-svg-scale
sbtext-rs INPUT OUTPUT --python-backend
sbtext-rs INPUT --emit-merged merged.sbtext
```

## Native + Library

- Native CLI remains the default workflow.
- Core compile logic is available from `src/lib.rs`, including:
  - `run_cli(...)`
  - `compile_entry_to_sb3_bytes(...)`
  - `compile_source_to_sb3_bytes(...)`

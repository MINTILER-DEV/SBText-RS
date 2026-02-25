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

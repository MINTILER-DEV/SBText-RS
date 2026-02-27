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

WASM library build (bindings enabled):

```bash
cargo build --target wasm32-unknown-unknown --features wasm-bindings --lib
```

## Usage

```bash
sbtext-rs INPUT OUTPUT
sbtext-rs INPUT OUTPUT --no-svg-scale
sbtext-rs INPUT OUTPUT --python-backend
sbtext-rs INPUT OUTPUT --allow-unknown-procedures
sbtext-rs INPUT --emit-merged merged.sbtext
sbtext-rs INPUT --emit-sbtc bundle.sbtc
sbtext-rs INPUT.sbtc OUTPUT.sb3
sbtext-rs INPUT OUTPUT --compile-sbtc
sbtext-rs INPUT.sb3 --decompile
sbtext-rs INPUT.sb3 OUT_DIR --decompile --split-sprites
```

## Native + Library

- Native CLI remains the default workflow.
- Core compile logic is available from `src/lib.rs`, including:
  - `run_cli(...)`
  - `compile_entry_to_sb3_bytes(...)`
  - `compile_source_to_sb3_bytes(...)`
  - `compile_sbtc_bytes_to_sb3_bytes(...)`
- WASM exports (feature-gated) are in `src/wasm.rs`:
  - `compile_source_to_sb3(...)`
  - `compile_source_to_sb3_with_options(...)`
  - `compile_sbtc_to_sb3(...)`
  - `compile_sbtc_to_sb3_with_options(...)`

## SBTC Bundle

- `.sbtc` is a compressed SBText compilation bundle.
- It contains:
  - merged SBText source (`merged.sbtext`)
  - merged SBText with origin markers (`merged_marked.sbtext`)
  - line origin map (`line_map.json`)
  - manifest (`manifest.json`)
- Build one from normal input with `--emit-sbtc`.
- Compile directly from `.sbtc` by using it as CLI input.

## SB3 Decompile

- `--decompile` converts `.sb3` to `.sbtext`.
- Without `--split-sprites`, output is a single `.sbtext` file (default: same name as input).
- With `--split-sprites`, output is a directory:
  - `main.sbtext` contains the stage block and `import` lines.
  - each sprite is written as its own `.sbtext` file.
  - costume assets referenced by `md5ext` are extracted beside the output files.

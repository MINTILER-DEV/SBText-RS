# sbtext-rs 1.1.1

Release date: February 26, 2026

## Highlights

- Added `when [key] key pressed` event support in SBText parser/codegen/decompiler.
- Added optional `--allow-unknown-procedures` mode in native CLI:
  - unknown procedure calls are allowed,
  - compiler emits warnings,
  - unknown calls compile as no-op `wait (0)` blocks.
- Fixed wasm32 compatibility by gating native CLI `main` for WASM builds.

## Included Changes (since 1.1.0)

- `181933d` fix: gate CLI main for wasm32 builds
- `558fef1` bug fixes, added when key pressed event

## Compatibility Notes

- Default behavior is unchanged: unknown procedures still fail semantic validation unless `--allow-unknown-procedures` is explicitly enabled.
- `--allow-unknown-procedures` is native-backend only and is not supported with `--python-backend` or `--decompile`.
- Existing `when flag clicked`, `when this sprite clicked`, and `when I receive [...]` event syntax remains unchanged.
# sbtext-rs 1.0.1

Release date: February 25, 2026

## Highlights

- Added a wasm-bindgen wrapper while preserving native CLI behavior.
- Exposed the compiler as a reusable Rust library API (`src/lib.rs`) so native and embedded use cases share the same core.
- Fixed pen color block emission so pen color parameter menu blocks are generated in the Scratch-compatible structure.
- Documented WASM build steps and exported bindings in README.

## Included Changes (since 1.0.0)

- `f00b64e` refactor: expose shared compiler library while keeping native CLI
- `ac65994` docs: note native+library dual-target setup
- `1a60a78` feat: add wasm-bindgen wrapper while preserving native CLI
- `ef201da` docs: document wasm build and exported bindings
- `ecd470a` fixed pen color blocks
- `44f1762` Cargo.lock

## Compatibility Notes

- Native CLI workflow remains unchanged.
- WASM bindings remain feature-gated behind `wasm-bindings`.
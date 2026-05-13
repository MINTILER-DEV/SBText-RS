# sbtext-rs 1.4.0

Release date: May 13, 2026

## Highlights

- Added SB3 project inspection from the CLI with `sbtext-rs inspect`.
- Added a built-in SB3 obfuscator with deterministic seeded output.
- Added advanced obfuscation passes for procedure wrapping and control-flow flattening.
- Refreshed the WASM package output in `pkg`.

## Included Changes

- Added shared SB3 archive read/write helpers that preserve `project.json` plus bundled assets.
- Added `sbtext-rs obfuscate INPUT.sb3 -o OUTPUT.sb3` CLI support.
- Added obfuscation levels, clicker preset support, and explicit pass selection flags.
- Added rename passes for variables, lists, broadcasts, and custom procedure proccodes.
- Added block ID randomization with recursive block reference rewriting.
- Added top-level layout scrambling for Scratch scripts.
- Added bait variable injection using original displaced project names where possible.
- Added MVP protected-variable support with fake variables and checksum metadata.
- Added procedure wrapping via generated forwarding custom blocks.
- Added control-flow flattening by extracting direct chains and substacks into generated helper procedures.
- Added core tests for archive preservation, renaming, ID rewriting, layout scrambling, clicker detection, wrapping, and flattening.

## CLI Examples

```bash
sbtext-rs inspect game.sb3
sbtext-rs obfuscate game.sb3 -o game.obf.sb3 --level high
sbtext-rs obfuscate game.sb3 -o game.obf.sb3 --preset clicker
sbtext-rs obfuscate game.sb3 -o game.obf.sb3 --wrap-procedures --flatten
```

## Compatibility Notes

- The obfuscator preserves non-`project.json` SB3 assets as-is.
- Output is deterministic when `--seed` is provided.
- Protected-variable mode is intentionally MVP-light and does not yet rewrite every arithmetic mutation into secure helper blocks.
- Control-flow flattening currently uses generated helper procedures rather than a full state-machine transformation so that output remains Scratch-safe.

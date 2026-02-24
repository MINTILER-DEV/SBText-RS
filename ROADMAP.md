# Rust Migration Roadmap

Current implementation is a transition layer:

- Native Rust CLI + import resolver/validator + codegen
- Optional Python backend bridge behind `--python-backend` for parity checks

## Next steps to become fully Python-free

1. Port lexer to Rust (`Token`, `LexerError`, Unicode/BOM handling).
2. Port parser + AST to Rust (matching current SBText grammar).
3. Port semantic analysis to Rust (scope tables, list/procedure validation).
4. [done] Port codegen to Rust (Scratch block JSON + asset packaging + SVG normalization).
5. [done] Replace Python backend bridge with native Rust backend as default.
6. [done] Keep Python backend behind `--python-backend` only for parity checks during migration.

## Parity strategy

- Use `tests/adv/*.sbtext` as smoke tests for both backends.
- Compare generated `project.json` structure and opcode correctness.
- Keep behavior-compatible exceptions for invalid imports and semantic failures.

# sbtext-rs 1.1.0

Release date: February 26, 2026

## Highlights

- Added native `.sb3 -> .sbtext` decompilation, including split-sprite output mode.
- Expanded control-flow coverage with `for each` and `while` support.
- Added broad opcode support needed by advanced decompiled game projects (motion/looks/sound/data/control blocks that were previously unsupported).
- Improved CLI progress reporting with live stage/task updates and per-step counters.
- Improved decompile/recompile compatibility for real-world projects (broadcast decoding, quoted custom block calls, zero-width Unicode handling, dotted decimal literals, and expression math reporter parsing like `ceiling(...)`).

## Included Changes (since 1.0.1)

- `ff2d8f8` feat: add sb3-to-sbtext decompiler with split-sprite mode
- `309c0ae` docs: add decompile CLI usage and split output notes
- `a514352` fix: quote unsupported bracket names in decompiled refs
- `40fa40a` fix: ignore zero-width unicode format chars in lexer
- `1c69a1f` fix: support multi-word and quoted procedure call names
- `ce8119d` fix: resolve project-wide vars/lists and tolerate decompiled call forms
- `7b56fb2` fix: broaden parser and lexer compatibility for decompiled syntax
- `4147e6d` fix: skip SVG costumes with non-positive viewBox dimensions
- `bb333d8` feat: add for-each and while control statements
- `75fe022` docs: document for-each and while syntax
- `f170ea8` fix: ensure unique costume entries per target
- `6932fb3` fix: improve decompiler reporter fidelity and add switch/list-contents support
- `b4947d1` docs: document switch statements and list contents reporter
- `ce81b47` feat: add CLI stage progress output for compile and decompile
- `6b7c59b` feat: add granular live progress updates for compile/decompile
- `9b5266f` feat: add support for advanced game unsupported opcodes
- `b79d630` fix: preserve broadcast names and relax keyword statement parsing
- `7414e14` fix: lex decimal literals that start with dot
- `03a7057` fix: quote decompiled custom block call names with symbols
- `f4e39eb` fix: parse math reporters like ceiling() in expressions

## Compatibility Notes

- Native CLI remains the default backend.
- Existing SBText source remains compatible; this release primarily improves decompile and edge-case parser/codegen compatibility.

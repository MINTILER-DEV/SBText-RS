# sbtext-rs 1.2.0

Release date: February 27, 2026

## Highlights

- Added variable and list declaration initializers:
  - `var score = 100`
  - `list inventory = ["potion", sword, 3]`
- Added sensing expression support for touching reporters in parser, semantic analysis, codegen, and decompiler:
  - `touching (expr)`
  - `touching sprite (expr)` / `touching object (expr)`
  - `touching color (expr)`
- Improved compile-time progress reporting with granular lexing, parsing, and semantic-check progress updates.
- Improved decompile fidelity:
  - preserves variable/list initial values when present in project JSON,
  - keeps key event names safely formatted in bracket syntax.
- Fixed remote procedure call discovery regression for qualified calls (cross-target RPC scaffolding now emits correctly).

## Included Changes (since 1.1.1)

- `72af0e9` feat/fix: initial values, touching reporters, and richer progress reporting
- `26e04f9` fix: stop skipping valid qualified procedure calls during remote call collection

## Compatibility Notes

- Existing declarations remain valid: omitted initializers still default to Scratch behavior (`0` for vars, `[]` for lists).
- List/variable initializer bare identifiers are treated as string values (for example, `sword` becomes `"sword"`).
- `touching` is now reserved as a keyword.

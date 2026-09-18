# sbtext-rs 1.5.0

## Highlights

- Added `length of (expr)` string-length reporter to expressions.
- Enables measuring the character length of any string expression (variables, literals, nested reports).
- Complements the existing `length of [list]` list-length reporter.

## Included Changes

- Added `Expr::StringLength` AST variant with a `text` operand.
- Added `length of (expr)` syntax to the parser for string length (uses `(` to distinguish from list `[` references).
- Added `operator_length` codegen support for the native Rust backend.
- Updated semantic analysis to validate expressions inside the string-length operand.
- Added `operator_length` decompiler support so string-length blocks round-trip to `length of (expr)`.
- Updated SYNTAX.md with the `length of (expr)` reporter documentation.

## Expression Examples

```sbtext
set [name] to ("Scratch")
say (length of (name))
say (length of ("hello world"))
say (join ("chars: ") with ((length of (answer))))
```

## Compatibility Notes

- `length of [list]` continues to produce list length (unchanged).
- `length of (expr)` produces string length via the Scratch `operator_length` block.
- Both forms are accepted in any expression context (e.g., `say`, `set`, nested reports).
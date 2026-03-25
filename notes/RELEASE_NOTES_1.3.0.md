# sbtext-rs 1.3.0

Release date: March 25, 2026

## Highlights

- Added `join` string concatenation operator to expressions.
- Enables building complex text strings from multiple sources in a single expression.
- Useful for data compression, logging, and dynamic text generation.

## Included Changes (since 1.2.1)

- Added `Expr::StringJoin` AST variant with `text1` and `text2` operands.
- Added `join (text1) with (text2)` syntax to parser for string concatenation.
- Added "join" keyword to lexer keyword set.
- Added `operator_join` codegen support for native Rust backend.
- Updated semantic analysis to validate expressions inside join operands.
- Updated SYNTAX.md with join reporter documentation.

## Expression Examples

```sbtext
set [result_text] to (join ("Hello ") with ("World"))
set [full_name] to (join [first_name] with [last_name])
say (join ("Score: ") with (score))
```

## Compatibility Notes

- String concatenation via `join` works with variables, lists, and nested expressions.
- Operates on Scratch string type, compatible with all expression contexts.
- Python backend support follows standard reporter handling patterns.

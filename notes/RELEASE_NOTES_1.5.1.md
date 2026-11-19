# Release Notes 1.5.1

- Preserve runtime expressions in `start sound`, `play sound ... until done`, and `key (...) pressed?` instead of replacing them with literal menu values. Literal menu blocks remain intact; numeric keycodes preserve their number type.
- Support the documented `e ^ (...)` and `10 ^ (...)` reporters through lexing, parsing, and native Scratch math code generation.
- Add focused regression coverage for sound selectors, key selectors, and exponential reporters.
- Add a reproducible Friday Night Funkin' SBText example with 61 tracks, 167 original difficulty charts, 10 story groups, menus, timing/scoring, holds, practice/botplay, and synchronized pause/resume. Media is fetched from pinned upstream sources and excluded from Git. Includes archive validation, a local Scratch VM player, and browser integration tests.

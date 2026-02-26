# SBText-RS Syntax Reference

This document describes the language accepted by the current Rust compiler (`sbtext-rs`) in this repository.

## 1) Compiler pipeline

`sbtext-rs` processes files in this order:

1. Import resolution (`import [SpriteName] from "relative/path.sbtext"`).
2. Lexing/tokenization.
3. Parsing into AST.
4. Semantic validation.
5. `.sb3` codegen (native Rust backend by default).

If you pass only `INPUT` with no `OUTPUT`, it still resolves imports, lexes, parses, and validates.

## 2) CLI

```bash
sbtext-rs INPUT OUTPUT
sbtext-rs INPUT OUTPUT --no-svg-scale
sbtext-rs INPUT OUTPUT --python-backend
sbtext-rs INPUT OUTPUT --allow-unknown-procedures
sbtext-rs INPUT --emit-merged merged.sbtext
```

Flags:

- `--no-svg-scale`: disables SVG normalization to `64x64`.
- `--emit-merged PATH`: writes merged source after import resolution.
- `--python-backend`: uses Python backend instead of native Rust backend (parity mode).
- `--allow-unknown-procedures`: allows unresolved procedure calls; unknown calls compile as no-op `wait (0)` and emit warnings.

## 3) Import system

Syntax (exact shape):

```sbtext
import [SpriteName] from "relative/path/to/file.sbtext"
```

Rules:

- Imports are only allowed at file top level (before any non-comment, non-blank code).
- Imported paths are resolved relative to the importing file.
- Imports are recursive.
- Circular imports are compile errors.
- Imported file must define exactly one sprite.
- Imported file must define no stage.
- Imported sprite name must match the `[SpriteName]` in the import statement.
- Duplicate sprite names in final merged project are compile errors (case-insensitive).

Import line notes:

- Leading/trailing spaces are allowed.
- End-of-line comments after an import are allowed.

## 4) Lexical rules

### 4.1 Whitespace and comments

- Spaces, tabs, and `\r` are ignored.
- Newlines are significant separators.
- `#` starts a comment to end of line.
- UTF-8 BOM (`U+FEFF`) is ignored.

### 4.2 Case behavior

- Keywords are case-insensitive (`Sprite`, `SPRITE`, `sprite` all parse as keyword `sprite`).
- Identifier matching in semantic checks is case-insensitive (targets/variables/lists/procedures).

### 4.3 Tokens

- Identifiers: start with letter/`_`; continue with letters, digits, `_`, `?`, and `.`.
- Numbers: ASCII digits, optional single decimal point.
- Strings: `"..."` with escapes `\"`, `\\`, `\n`, `\r`, `\t`.
- Operators: `+ - * / % = == != < <= > >=`.
- Delimiters: `(` `)` `[` `]` `,`.

### 4.4 Reserved keywords

`add all and answer ask at backdrop bounce broadcast by change clicked contains contents costume define delete direction each edge else end flag floor for forever go hide i if in insert item key left length list mouse move next not of on or pick point pressed random receive repeat replace reset right round say seconds set show size sprite stage steps stop switch then think this timer to turn until var wait when while with x y`

## 5) File and target structure

Top level after imports:

- `sprite ... end`
- `stage ... end`

At least one target is required.
At most one stage is allowed.

Target members:

- `var <name>`
- `list <name>`
- `costume "relative/or/absolute/path.svg|.png"`
- `define ... end`
- `when ...` scripts

Example:

```sbtext
stage
  var score
end

sprite Player
  var hp
  list inventory
  costume "assets/player.svg"
end
```

Notes:

- `stage` name is optional (`stage` defaults to name `Stage`).
- `sprite stage` is accepted and becomes sprite name `Stage`.

## 6) Events

Supported event headers:

- `when flag clicked`
- `when this sprite clicked`
- `when I receive [message]`
- `when [key_name] key pressed`

Event body is a statement sequence.
Event `end` is optional in some layouts, but using explicit `end` is recommended for clarity.

## 7) Statements

All currently supported statement forms:

### 7.1 Broadcast / timing

```sbtext
broadcast [message]
broadcast and wait [message]
wait (seconds_expr)
wait until <condition_expr>
```

### 7.2 Variables

```sbtext
set [var_name] to (expr)
change [var_name] by (expr)
```

Also supported:

```sbtext
set x to (expr)
set y to (expr)
set size to (expr)
change x by (expr)
change y by (expr)
change size by (expr)
```

### 7.3 Motion / looks

```sbtext
move (expr) [steps]
turn right (expr)
turn left (expr)
go to x (expr) y (expr)
point in direction (expr)
if on edge bounce

say (expr)
say (expr) for (expr) [seconds]
think (expr)
show
hide
next costume
next backdrop
switch costume to (expr)
switch backdrop to (expr)
```

`move (expr) steps` is also accepted.

### 7.4 Control flow

```sbtext
repeat (expr)
  ...
end

for each [var_name] in (expr)
  ...
end

while <condition_expr>
  ...
end

repeat until <condition_expr>
  ...
end

forever
  ...
end

if <condition_expr> then
  ...
else
  ...
end
```

### 7.5 Stop / sensing

```sbtext
stop (expr)
ask (expr)
reset timer
```

`ask (expr)` compiles to Scratch `ask and wait`.

### 7.6 Lists

```sbtext
add (expr) to [list]
delete (expr) of [list]
delete all of [list]
insert (expr) at (expr) of [list]
replace item (expr) of [list] with (expr)
```

### 7.7 Procedure calls

```sbtext
local_proc (arg1) (arg2)
SpriteName.proc_name (arg1) (arg2)
```

Cross-target calls use `SpriteName.proc_name`.

### 7.8 Pen extension

```sbtext
pen down
pen up
erase all
stamp

set pen size to (expr)
change pen size by (expr)

set pen color to (expr)
change pen color by (expr)

set pen saturation to (expr)
change pen saturation by (expr)

set pen brightness to (expr)
change pen brightness by (expr)

set pen transparency to (expr)
change pen transparency by (expr)
```

## 8) Procedures

Definition:

```sbtext
define procName (param1) (param2)
  ...
end
```

Shorthand warp definition:

```sbtext
define !procName (param1) (param2)
  ...
end
```

Run-without-screen-refresh definition:

```sbtext
define procName (param1) (param2) run without screen refresh
  ...
end
```

Rules:

- Local procedure calls are validated for existence and argument count.
- Local procedure calls before the definition line are compile errors.
- Duplicate procedure names in the same target are compile errors.
- Duplicate parameter names are compile errors.
- `run without screen refresh` maps to Scratch custom block warp mode.
- `define !name (...)` is shorthand for warp mode.

## 9) Expressions

### 9.1 Primary expressions

- Numbers: `1`, `3.14`
- Strings: `"hello"`
- Variables: `score`, `[score]`
- Cross-target variable read: `SpriteName.varName`
- Grouping: `(expr)`

### 9.2 Reporters / built-ins

```sbtext
pick random (a) to (b)
item (index) of [list]
length of [list]
contents of [list]
[list] contains (expr)
key (expr) pressed?
answer
mouse x
mouse y
timer
floor (expr)
round (expr)
```

`key (expr) pressed` (without `?`) is also accepted.

### 9.3 Unary/binary operators

Unary:

- `-expr`
- `not expr`

Binary:

- Arithmetic: `+ - * / %`
- Comparison: `= == != < <= > >=`
- Boolean: `and or`

Precedence (low to high):

1. `or`
2. `and`
3. `= == != < <= > >=`
4. `+ -`
5. `* / %`

Procedure calls are not allowed inside expressions.

## 10) Name forms and fields

### 10.1 Bracket text fields

Used by broadcast/variable/list forms:

- `[message]`
- `[var_name]`
- `[list_name]`

Inside bracket fields, tokens are joined with spaces.
Variable field names also allow `[var myName]` (leading `var` token is stripped).

### 10.2 Qualified names

`Target.member` is parsed as a single identifier token containing a dot.
Do not put spaces around the dot.

## 11) Semantic validation rules

Current semantic checks include:

- Project must contain at least one target.
- At most one stage.
- Duplicate target names rejected.
- Duplicate variable names per target rejected.
- Duplicate list names per target rejected.
- Unknown variable references rejected.
- Unknown list references rejected.
- Unknown procedure calls rejected (unless `--allow-unknown-procedures` is enabled).
- Local call before local definition rejected.
- Procedure argument count mismatch rejected.
- Cross-target procedure target/procedure/arg-count validated.
- Cross-target variable target/variable existence validated.
- Variable blocks (`set [x]`, `change [x]`) cannot target procedure parameters.
- Empty broadcast message rejected.

## 12) Codegen behavior notes

### 12.1 Cross-target procedure calls

`Target.proc(args...)` compiles into generated RPC plumbing:

1. Generated global variables for arguments.
2. A generated broadcast message.
3. `broadcast and wait`.
4. A generated handler in callee target that invokes the local procedure.

This guarantees wait-until-finished semantics for cross-target calls.

### 12.2 Cross-target variable reads

`Target.var` compiles to Scratch `sensing_of`.

### 12.3 Costume assets

- Supported formats: `.svg`, `.png`.
- If target has no costume, compiler injects a default SVG costume/backdrop.
- SVGs are normalized to `64x64` by default (`--no-svg-scale` disables this).
- With scaling enabled, sprite rotation center is set to `(32, 32)`.
- With scaling disabled, center is `(width/2, height/2)` from SVG bounds.

## 13) Known sharp edges

- `if` conditions are parsed up to `then`; keep them on one line for predictable behavior.
- `wait until` and `repeat until` conditions are read until newline.
- `stop (expr)` uses literal text for stop option; non-literal expressions default to `"all"` in codegen.
- Procedure names declared as quoted strings are parsed, but call syntax expects identifier-style names; stick to identifier procedure names.

## EXTRA INFORMATION

- Scratch operates in 30fps. Adding a 0.0333 wait in a forever loop makes it slower than 30fps.

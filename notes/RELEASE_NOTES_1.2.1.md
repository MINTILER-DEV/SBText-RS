# sbtext-rs 1.2.1

Release date: March 25, 2026

## Highlights

- Added `.sprite3` export support from SBText projects.
- Added sprite selection for `.sprite3` export:
  - auto-select when exactly one sprite exists,
  - choose with `--sprite-name <NAME>`,
  - interactive prompt when multiple sprites exist in terminal mode.
- Upgraded manual GitHub workflow to build binaries and create a release from a version input.

## Included Changes (since 1.2.0)

- Added `.sprite3` codegen path (`sprite.json` + selected sprite assets only).
- Added CLI support for `.sprite3` export and `--sprite-name`.
- Added manual workflow input `version` and automatic tag normalization (`v<version>`).
- Workflow now:
  - resolves `notes/RELEASE_NOTES_<version>.md` when present,
  - generates release notes when missing,
  - builds Windows/Linux/macOS release binaries,
  - creates/updates the GitHub release and uploads artifacts.

## Compatibility Notes

- `.sb3` and `.sbtc` compile workflows are unchanged.
- `.sprite3` export is native backend only (not `--python-backend`).
- For non-interactive environments with multiple sprites, pass `--sprite-name`.

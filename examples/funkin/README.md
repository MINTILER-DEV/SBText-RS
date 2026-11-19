# Friday Night Funkin' in SBText

A playable, unofficial FNF remake compiled entirely to ordinary Scratch 3 blocks by **sbtext-rs**. It uses the original upstream charts, music, characters, notes, fonts, and stage props rather than generated lookalikes.

The pinned catalog contains **61 tracks, 167 difficulty charts, 90,437 notes, 43 character sets, 21 stage backgrounds, and 10 story groups**: Tutorial, Weeks 1–7, Weekend 1, and the bonus collaboration. Erect, Nightmare, Pico mixes, and BF variations are available where upstream provides them.

## Play

Open `build/Funkin.sb3` in Scratch or TurboWarp. For the included local browser player:

```sh
npm install
npm run play
```

Open the localhost address printed by the server and click **Load & Play**. This is a large project; initial loading includes decoding the complete soundtrack.

| Action | Keys |
| --- | --- |
| Four gameplay lanes, left to right | Left/Down/Up/Right, A/S/W/D, or H/J/K/L |
| Navigate menus | Up/Down |
| Change difficulty/options | Left/Right |
| Select / resume | Enter |
| Pause / resume in Scratch | P |
| Pause / back in the included browser player | Esc |
| Retry from pause/results | R |
| Return from pause | Space |
| Toggle botplay in Freeplay | B |

Long notes must be held through the tail. Pause takes effect at the next audio segment boundary, within about a second; resume continues from that boundary. The browser player maps Esc to P for compatibility with Scratch's keyboard device. Use Esc to leave freeplay/options in the browser, or P in Scratch.

Options include downscroll, ghost tapping, practice, botplay, timing offset in milliseconds, and volume. Practice and botplay never write ranked scores. Scores persist for the current project session and are included if the project is saved from a Scratch editor.

## Build from source

Requirements: Python 3.10+, Pillow, FFmpeg (including its IMA ADPCM encoder), Rust/Cargo. Node.js is needed only for the local player and browser tests.

From the compiler repository root:

```sh
cargo build --release
python3 -m pip install Pillow
python3 examples/funkin/tools/fetch.py
python3 examples/funkin/tools/prepare.py
python3 examples/funkin/tools/visuals.py
python3 examples/funkin/tools/build.py
```

The first build downloads upstream media; subsequent builds use the verified cache. `upstream.lock.json` pins all three reference repositories to exact commits and Git blob hashes. Downloaded content, converted assets, dependencies, and builds are excluded from Git.

Edit `src/engine.sbtext` and `src/renderer.sbtext`. The build injects chart lists and asset declarations into `build/funkin.sbtext`, then compiles it to `build/Funkin.sb3`. The engine remains SBText; Python only prepares data/assets and invokes the compiler. No JavaScript gameplay, custom Scratch extensions, or `.sb3` postprocessing are needed to play.

The importer supports Sparrow XML, Animate texture atlases (including nested affine transforms and rotated parts), and the legacy packed Spirit sheet. Original instrumental and vocal stems are mixed to mono 22.05 kHz. Short IMA ADPCM WAV segments use Scratch's own decoder, avoiding the encoder delay that MP3 segments introduce at every seam. Segment start times come from the actual encoder packet boundaries. The game uses a single monotonic song clock and compensates for BPM changes.

`prepare.py --base-only` prepares the 27 default tracks; `--song bopeebo` can prepare a smaller development build. Full catalog assertions in the integration test intentionally target the complete default build.

## Validation

```sh
cargo test --all-targets
cd examples/funkin
npm install
npx playwright install chromium
npm test
npm run test:full-audio
```

The build validates all embedded asset references and block graph links. Browser integration runs the actual Scratch VM: real keyboard navigation, note hits, PBOT1 scoring, holds/drops, late misses, pause/resume, every chart load, story progression, game over, practice mode, and ranked-score gating. Screenshots are written to `build/screenshots/`.

The deterministic test fixture retains all engine code, charts, and art but omits soundtrack decoding. The `test:full-audio` pass loads the complete `.sb3`, verifies every audio segment decodes, measures seam durations against the song clock (under 2 ms mismatch), checks the decoded soundtrack is audible, and repeats the runtime assertions.

## Scope

This is a Scratch adaptation with its own menus and screen composition. Character poses use three sampled frames per animation. Stage props are flattened into backgrounds; chart BPM, camera focus, and scroll-speed changes are supported. Scripted video/dialogue cutscenes, full per-song cinematic event choreography, the original chart editor, online services, and the original engine's complete animation timelines are outside this port. In particular, collaboration-specific cinematic events are not recreated.

## Original work and references

- [FunkinCrew/Funkin](https://github.com/FunkinCrew/Funkin): `Conductor.hx`, `Constants.hx`, and `play/scoring/Scoring.hx` informed timing, health, the 160 ms hit window, judgement thresholds, and PBOT1 score curve.
- [FunkinCrew/funkin.assets](https://github.com/FunkinCrew/funkin.assets): original charts/metadata, levels, voices/instrumentals, atlas exports, backgrounds, UI, and VCR font.
- [FunkinCrew/funkin.art](https://github.com/FunkinCrew/funkin.art): original art references and character icon references.

See [THIRD_PARTY.md](THIRD_PARTY.md). The original content is owned by The Funkin' Crew and its credited contributors. Generated media is kept local and is not included in the source commit.

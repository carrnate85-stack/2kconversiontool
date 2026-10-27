# Headless player preview

Renders real NBA 2K27 players outside the app window. You can compare a
baseline renderer with your working tree side by side, without the Python engine.

1. **extractor/** is a small Rust tool on `shared/nba2k-assets`. It reads the game
   manifest and writes a player's IFFs (decompressed) into a folder shaped like a
   staged player. Game files are only read.
2. **decode-player.cjs** runs the app binary headless (`--serve-rpc`) to prepare
   the player's native preview resources. It then runs the bundled
   `frontend/preview-worker.bundle.js` in Node, just as `loadStaged()` in
   `web/tauri-bridge.ts` does, and writes the preview payload as JSON.
3. **harness/** loads that payload into two explorer renderers: `old/` from a git
   ref and `new/` from the working tree. It applies the same packed-normal decoding
   and seam pairing as the app wrapper.

## Usage

Run from `character-mod-tool-tauri/`. `$GAME` is the NBA 2K27 install folder.

```bash
cargo build --release
cargo build --release --manifest-path tools/headless-preview/extractor/Cargo.toml
```

```bash
tools/headless-preview/extractor/target/release/player-extract list "$GAME" '^char/sig/png\d+\.iff$' 20
```

```bash
tools/headless-preview/extractor/target/release/player-extract player "$GAME" 00110 tools/headless-preview/work/players/png00110
```

```bash
node tools/headless-preview/decode-player.cjs --game "$GAME" tools/headless-preview/work/players/png00110/png00110.iff
```

```bash
node tools/headless-preview/harness/prepare.cjs --baseline HEAD
```

```bash
node tools/headless-preview/harness/serve.cjs 8790
```

Then open `http://localhost:8790/?player=png00110&jersey=0&focus=head`.

For a color reference, extract the game's own headshot for a player. It's
`player_images/chr_g####.iff`, and the number matches the png ID; not every
player has one.

```bash
tools/headless-preview/extractor/target/release/player-extract headshot "$GAME" 809 tools/headless-preview/harness/players/headshot-809.dds
```

Add `&headshot=809` to the harness URL to show it next to the renders. Headshots
are photographs, so compare brightness and hue rather than expecting identical
lighting.
`player` prints the player's gender, skin tone and iris color, so you know what
you extracted.

To look inside any manifest IFF, `members` lists its members (optionally filtered
by a regex), `member` prints one as JSON (TXTR, SCNE and other trees), and
`buffer` writes a scene vertex or index buffer decompressed to its SCNE `Size`:

```bash
tools/headless-preview/extractor/target/release/player-extract members "$GAME" char/fpglobal.iff 'normal_avg'
```

```bash
tools/headless-preview/extractor/target/release/player-extract member "$GAME" char/fpglobal.iff chest_normal_avg_ripped.TXTR
```

`decode-player.cjs` options, also settable as environment variables:
`--game` / `NBA2K27_ROOT`, `--exe` / `CMT_EXE` (default
`target/release/character-mod-tool-tauri.exe`) and `--data` / `CMT_TEST_DATA`
(default `work/cmt-data`). It writes its own `settings.json` into that isolated
data folder, so your real app settings are never used. `--config <name>` picks
an accessory configuration, such as `flat_top`, which loads that hair. The config
names are the `png#####_config_<name>.iff` files the extractor wrote. Output
defaults to `harness/players/<player>[-<config>].json`.

Rerun `prepare.cjs` after editing `web/renderer/`. To compare against an older
renderer, pass that commit as `--baseline`. Once your changes are committed,
`HEAD` would show the same renderer on both sides.

### Harness URL parameters

| Parameter | Effect |
| --- | --- |
| `player=png00110` | Load `players/png00110.json`; without it, a synthetic head that needs no game files |
| `jersey=0` | Hide the preview jersey and shorts |
| `focus=head`, `headY`, `headZoom` | Head close-up: target height below the top, and distance, as fractions of model height |
| `focus=eyes`, `eyeZoom` | Aim at the eye geometry; distance as a multiple of the eyes' extent |
| `yaw`, `pitch`, `zoom` | Orbit angles in radians; `zoom` multiplies the fitted distance |
| `only=old` / `only=new` | Load one side |
| `headshot=809` | Show `players/headshot-809.dds` (the game's headshot) as a reference panel |
| `iris=green`, `eyerange=1` | Synthetic head only: iris color, and a range-packed eye texture |

`measureDetail('old' | 'new', [x, y, w, h])` in the page console returns a fine-detail score (mean absolute Laplacian of luminance) for a canvas region, to compare sides by number rather than by eye.

## Notes

- If `cl.exe` fails with `0xc0000017` while building, `PATH` is too long (over
  ~32K characters). Build from a shell with a de-duplicated `PATH`.
- These previews show the neutral rest pose: the scene's morph targets are
  facial expressions and pose correctives, all zero at rest. They also skip the
  wrapper's jersey/bust fit morphs and the default config's accessories.
- Everything this writes (`work/`, `harness/players/`, the assembled renderers)
  is game-derived or generated and is git-ignored.

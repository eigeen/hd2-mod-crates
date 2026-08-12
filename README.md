# hd2-migrator (Rust)

Rust port of the Python armor migrator tool. Takes a Helldivers 2 armor
mod patch (the `9ba626afa44a3aa3.patch_0` trio) and re-keys it to every other
armor archive, producing one ready-to-drop variant per target.

## Status

| Path | State |
|---|---|
| Game-data migration | ✅ authoritative Unit-part mapping, geometry fallback, and per-target migration |
| LEGACY package read/write | ✅ |
| DSAR (LZ4 block) decompression | ✅ |
| Slim `bundles.*.nxa` reassembly | ✅ |
| Empty-mesh padding (sanitized + verbatim) | ✅ |
| Interactive CLI fill-in (no-args UX) | ✅ |
| Parallelism (rayon, per-target) | ✅ |
| Web/WASM appearance migration (armor + helmet) | ✅ |
| Web/WASM Unit version repatching | ✅ |

The migrator derives FileID remaps directly from game archives. Major Unit
parts are matched first through the bundled manually verified
`armor_mappings.merged.json`; geometry and customization-name matching only
fill gaps that the authoritative table cannot cover.

The Web/WASM UI supports separate armor and helmet appearance migration. Armor
uses the multi-part table and geometry fallback; helmet migration uses its
dedicated one-Unit mapping and only reads archive TOCs for lower I/O and memory
use. The UI can also refresh a mod patch's Unit version-dependent layout data
from the currently installed game without modifying the game files.

## Credits

Unit repatching behavior was inspired by hd2-repatcher, created by Evie / RaidingForPants. This project provides an independent Rust/WASM implementation.

See [RaidingForPants/hd2-repatcher](https://github.com/RaidingForPants/hd2-repatcher).

Armor and helmet mapping tables provided by [@大紫](https://space.bilibili.com/263230957).

## Build

```
cargo build --release
./target/release/hd2-migrator --help
```

## Usage

### Migrate with the game `data/` directory

```
hd2-migrator \
  --patch  path/to/your_mod/9ba626afa44a3aa3.patch_0 \
  --data-dir /path/to/Helldivers_2/data \
  --out-dir out/
```

Use `--armor-mapping-json path/to/armor_mappings.merged.json` to test an
updated authoritative table without rebuilding the binary.

### Interactive mode

Run with no arguments to be prompted for the patch path, game data directory,
category, targets, and output directory:

```
hd2-migrator
```

The `--non-interactive` flag turns missing required args into fatal errors
rather than prompts.

### Flags

See `--help` for the full list.

```
--patch                       Source mod patch file
--out-dir                     Output directory
--data-dir                    Game data/ directory
--armor-mapping-json          Override authoritative Unit-part mapping
--source                      Override source armor hash
--target a,b,c                Limit to these target hashes/names
--category                    archivehashes.json category (default: Armor)
--index                       Override archivehashes.json path
--empty-mesh-from             Custom empty Unit patch (else builtin)
--no-padding                  Disable padding extras with empty meshes
--empty-mesh-verbatim         Keep template bytes as-is (no sanitization)
--patch-suffix                Output filename inside each target dir
--experimental-partial-remap  Allow incomplete Unit remap
--non-interactive             Fail rather than prompt for missing args
-v / -vv / -vvv               INFO / DEBUG / TRACE logging
```

## Embedded assets

Five assets are embedded into the binary at build time via `include_bytes!` /
`include_str!`:

- `assets/archivehashes.json` — armor hash → name index (copied verbatim from
  the Python package).
- `assets/armor_mappings.merged.json` — manually verified armor name →
  major Unit part → FileID table.
- `assets/helmet_mappings.json` — helmet name → Helmet Unit FileID table.
- `assets/empty_mesh/{toc,gpu,stream}.bin` — single-vertex empty mesh used as
  the default padding template.
- `../hashlists/bonehash.txt` — mesh group name hashes used for Unit
  customization fallback.

`build.rs` asserts all three are present and non-empty (stream is allowed empty
for the default 1-vertex mesh) so a missed extraction fails the build loudly.

### Re-extracting `empty_mesh/*.bin` from the Python source

If you need to regenerate the empty mesh assets from the upstream
`_builtin_empty_mesh.py`:

```bash
python3 -c "
import sys, pathlib
sys.path.insert(0, '.')
from mod_armor_migrator._builtin_empty_mesh import TOC_DATA, GPU_DATA, STREAM_DATA
d = pathlib.Path('mod_armor_migrator_rs/assets/empty_mesh')
d.mkdir(parents=True, exist_ok=True)
(d/'toc.bin').write_bytes(TOC_DATA)
(d/'gpu.bin').write_bytes(GPU_DATA)
(d/'stream.bin').write_bytes(STREAM_DATA)
"
```

Run from the workspace root.

## Testing

`cargo test` runs ~22 unit tests covering:

- murmur64/32 vectors (regression-pinned against the TypeID constants).
- Archive LEGACY round-trip + minimum-size padding + 64B alignment.
- FileID / SlotID rewrite (Unit header refs, MaterialIDs, whole-blob u32 scan,
  Material TexIDs).
- Customization name extraction (regex-free byte scanner).
- Builtin empty mesh template loads + sanitized audit is non-drawing.
- DSAR header magic validation.
- authoritative armor mapping parsing and major Unit-part assignment.
- archivehashes.json schema parsing.

There are no end-to-end integration tests against real game data — that
requires a game install and is left to manual verification.

## Layout

```
src/
  archive/       LEGACY package + DSAR decompression
  cli/           clap args, dialoguer fill-in, tracing, indicatif progress
  migrator/      game-data orchestration (mode_a.rs)
  padding/       empty Unit template + sanitize + pad_patch
  unit/          authoritative mapping, names, semantic key match, geometry
  refs.rs        FileID + SlotID rewrite inside Unit/Material blobs
  hashing.rs     murmur64 + murmur32 (high-32 truncation)
  constants.rs   TypeID + magic constants + alignment helpers
  error.rs       eyre type aliases + typed error enum
  index.rs       archivehashes.json loader (builtin via include_str!)
```

## Differences from the Python tool

- All printing routed through `tracing`; no direct stdout/stderr from the
  library. CLI prints a colored summary at the end.
- Per-target migration runs in parallel via `rayon::par_iter`.
- Empty-mesh assets are embedded into the binary (no runtime base64 decode).
- Interactive mode (dialoguer) is added — Python is flags-only.
- `--non-interactive` flag added (fails fast in CI / scripts).

---
id: v1.2.0
sequence: 4
version: "1.2.0"
releasedAt: "2026-09-09"
locale: en
title: Version 1.2.0
---

# Version 1.2.0

## Head Merge

Added Head Merge, a tool for combining a helmet model with a body model.

*After merging, install a separate helmet-removal Mod to hide the game's original helmet.*

## Merge Mods

- Added Merge Mods to the desktop app. Combine multiple Patch sets into one Mod and adjust which takes priority when resources conflict.

## Appearance Migration and Update Mod Version

- Improved part matching during appearance migration. When direct matching is unavailable, the tool uses the original game equipment's model to reduce missing and incorrect matches.
- Fixed an issue where migrating to FS-05 replaced the Democracy Officer instead.
- Improved legacy model repair in Update Mod Version, handling versions and legacy vertex formats, and updating LOD data where required by the version transition.
- Significantly sped up game-file scanning when updating Mods.

Run Update Mod Version separately to apply model version repairs. Appearance migration and Mod merging do not apply these repairs automatically.

## Stability and Usability

- Reduced memory use when importing and merging large Mods in the web app, lowering the risk of out-of-memory failures.
- Fixed read failures caused by incorrect resource metadata in some Mods. Missing or incomplete companion files are reported directly.
- Fixed incomplete ZIP output in the desktop app.
- Aligned tool interfaces, task reports, and error messages across the web and desktop apps, and improved responsiveness during progress updates.
- Advanced options now live in a separate menu for a simpler interface.
- Added a desktop download entry to the web app, with recommendations for larger jobs or browser limitations.

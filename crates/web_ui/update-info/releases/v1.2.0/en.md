---
id: v1.2.0
sequence: 4
version: "1.2.0"
releasedAt: "2026-08-24"
locale: en
title: Version 1.2.0
---

# Version 1.2.0

## Update Mod Version

- Restored the complete legacy Unit update flow from the original repatcher. The updater now checks the Unit version, legacy vertex format, and LOD data instead of changing only the version number.
- The verified `10800437 → 10800438` path changes only the Unit version and legacy vertex format. Other legacy versions pass safety checks before their LOD group is synchronized from current game data.
- When parts cannot be matched directly, the updater now uses geometry from the Mod's original equipment as a fallback, reducing missing or incorrect matches.
- Removed the experimental culling option and its rewrite path. The current fix treats the issue as a Unit version and structure upgrade and does not rewrite culling data.

These changes apply only to “Update Mod Version.” Appearance migration and manual Mod merging do not run the Unit update automatically, and GPU and stream files remain unchanged.

## Compatibility and Stability

- When several archives contain resources with the same name, the tool now prefers the verified playable FS-05 data instead of selecting an unusable copy.
- The web and desktop apps now share the same Patch sidecar validation and report missing required files directly.
- Fixed desktop ZIP output that could finish before all data was written.

## Usability

- Advanced options now live in a separate menu, keeping common actions simpler.
- The web app recommends the desktop app for larger jobs and work better suited to local processing.
- Revised Chinese and English wording to make the scope of migration, updating, and merging clearer.

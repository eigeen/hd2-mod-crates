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

- Completed the legacy Unit update flow. It now checks and fixes the Unit version, legacy vertex format, and LOD data.
- For `10800437 → 10800438`, only the Unit version and legacy vertex format are updated. Other legacy versions use current game data to update the LOD group after safety checks pass.
- If a part cannot be matched directly, geometry from the Mod's original equipment is used to reduce missing and incorrect matches.
- Removed the experimental culling option and related code. The issue comes from Unit version and structure changes, so culling data is not modified.

These fixes run only with “Update Mod Version.” Appearance migration and Mod merging are unaffected, and GPU and stream files are not modified.

## Compatibility and Stability

- When game resources share a name, the tool prefers verified FS-05 data and avoids unusable copies.
- The web and desktop apps now use the same Patch sidecar checks. Missing files are reported directly.
- Fixed incomplete ZIP output in the desktop app.

## Usability

- Advanced options now live in a separate menu for a simpler interface.
- The web app recommends the desktop app for larger jobs.
- Revised Chinese and English text to clarify the scope of migration, updating, and merging.

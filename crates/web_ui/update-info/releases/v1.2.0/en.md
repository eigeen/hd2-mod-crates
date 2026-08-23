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
- If a part cannot be matched directly, geometry from the Mod's original equipment is used to reduce missing and incorrect matches.

These fixes run only with “Update Mod Version.” Appearance migration and Mod merging are unaffected.

## Compatibility and Stability

- Added resource overrides to prevent the wrong resource from being selected when game resources share a name. This fixes an issue where some equipment migrations incorrectly used the Democracy Officer as the target.
- The web and desktop apps now use the same Patch sidecar checks. Missing files are reported directly.
- Fixed incomplete ZIP output in the desktop app.

## Usability

- Advanced options now live in a separate menu for a simpler interface.
- The web app recommends the desktop app for larger jobs.

# HD2 Mod Tools — Desktop

The desktop frontend mirrors `crates/web_ui`, but performs patch inspection,
migration, and Unit repatching in native Rust instead of WebAssembly.

## Architecture

- Shared components, theme, assets, mapping rules, and translations come from
  the `@hd2-mod-tools/migrator-ui` workspace package in `crates/migrator_ui`.
- `src/desktopClient.ts` is the small native IPC adapter.
- `src-tauri/src/migration.rs` owns the desktop commands.
- `src-tauri/src/migration/patch.rs` loads and validates a patch trio.
- `src-tauri/src/migration/output.rs` writes output ZIPs transactionally.
- Large patch and game files never cross the IPC boundary.

## Verification

From this directory:

```powershell
bun run build
bun run desktop build
bun run smoke
bun run exe:info
```

`bun run desktop build` generates the packaged release executable without an
installer because `bundle.active` is disabled by default. `bun run smoke`
launches that executable, verifies the rendered
WebView, native IPC against the real-data fixture, and single-instance behavior,
then closes the process it started. Run `bun run desktop build` first; a plain
`cargo build --release` keeps Tauri's development URL and is not a packaged app.
`bun run exe:info` verifies the shared version and prints the EXE revision, size,
and SHA-256 checksum.

From the workspace root:

```powershell
cargo test -p hd2_migrator_desktop
cargo clippy -p hd2_migrator_desktop --all-targets --no-deps
```

The release executable is written to
`target/release/hd2_migrator_desktop.exe`.

## Signed desktop releases

`.github/workflows/release-desktop.yml` runs only for stable `v<major>.<minor>.<patch>`
tags. Prerelease suffixes are deliberately rejected so a beta cannot become the
latest stable client update. The workflow builds Windows x64 without access to
the signing key, then a protected signing job signs, verifies, and uploads this
archive to a draft GitHub Release:

```text
hd2-mod-tools-desktop-x86_64-pc-windows-msvc-v<version>.zip
```

The draft Release body is generated from the matching localized update
changelogs under `crates/web_ui/update-info/releases/v<version>/`. YAML front
matter is removed, then the Chinese and English Markdown bodies are included in
that order. Rerunning a draft release may replace its signed asset and body; the
workflow refuses to modify an already published Release.

Create a GitHub Environment named `desktop-signing` if signing should require
reviewer approval. Store both key values as Repository Secrets, and generate
the signing key once on a trusted machine with an access-controlled offline
backup:

```powershell
cargo install zipsign --version 0.2.1 --locked
zipsign gen-key desktop-update-private.key desktop-update-public.key
$privateKeyBase64 = [Convert]::ToBase64String(
    [IO.File]::ReadAllBytes((Resolve-Path -LiteralPath 'desktop-update-private.key'))
)
$publicKeyHex = [Convert]::ToHexString(
    [IO.File]::ReadAllBytes((Resolve-Path -LiteralPath 'desktop-update-public.key'))
).ToLowerInvariant()
$privateKeyBase64 | gh secret set ZIPSIGN_PRIVATE_KEY_BASE64
$publicKeyHex | gh secret set ZIPSIGN_PUBLIC_KEY_HEX
```

The public key Secret is embedded in the binary during the keyless build.
The signing job derives the public key from the protected Secret and refuses to
sign unless it exactly matches `ZIPSIGN_PUBLIC_KEY_HEX`. Never replace either
value as a routine CI repair: clients containing the old public key will reject
later releases. Key rotation requires a planned transition release that trusts
both keys and archives signed by both keys.

Before releasing, update the workspace and desktop package versions to the same
semantic version. Then push the matching tag:

```powershell
git tag v1.2.3
git push origin v1.2.3
```

Review and publish the generated draft Release manually. Draft releases are not
offered to installed clients. Once published, the updater selects the archive
whose Rust target triple matches the running binary, verifies the embedded ZIP
signature, replaces the executable, and restarts the application.

The first release containing this updater cannot update versions that predate
it, so distribute that release manually once. Users must extract the executable
to a persistent writable directory before running it. Updating from a ZIP
preview, a temporary directory, `Program Files`, or another read-only/managed
location is unsupported.

### Release checklist

Before publishing the draft:

1. Confirm the x64 ZIP asset exists and contains only `hd2_migrator_desktop.exe`.
2. Download the asset and verify it with the release public key and the
   `update-archive-verifier` built from this workspace.
3. On a real Windows x64 machine, upgrade the previous published version and
   confirm signature verification, replacement, restart, and the displayed
   version.
4. Confirm the Release is a full release, not a prerelease, and publish it only
   after the signing Environment reviewer approves the run.

Do not move a published tag or rerun the workflow to replace published assets;
publish a new patch version instead. Enable GitHub immutable releases when the
repository supports them. Keep a record of the public-key fingerprint, offline
private-key backup, authorized releasers, and the response procedure for a lost
or exposed key. The ZIP signature protects the updater transport but is not a
Windows Authenticode signature; add Authenticode before ZIP signing if a trusted
Windows publisher identity and reduced SmartScreen friction are required.

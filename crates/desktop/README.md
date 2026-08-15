# HD2 Mod Tools — Tauri

The desktop frontend mirrors `crates/web_ui`, but performs patch inspection,
migration, and Unit repatching in native Rust instead of WebAssembly.

## Architecture

- Shared Web UI components, theme, mapping rules, and translations are imported
  from `crates/web_ui/src`.
- `src/nativeClient.ts` is the small Tauri IPC adapter.
- `src-tauri/src/migration.rs` owns the desktop commands.
- `src-tauri/src/migration/patch.rs` loads and validates a patch trio.
- `src-tauri/src/migration/output.rs` writes output ZIPs transactionally.
- Large patch and game files never cross the IPC boundary.

## Verification

From this directory:

```powershell
bun run build
bun run tauri build
```

From the workspace root:

```powershell
cargo test -p hd2_migrator_tauri_ui
cargo clippy -p hd2_migrator_tauri_ui --all-targets --no-deps
```

The release executable is written to
`target/release/hd2_migrator_tauri_ui.exe`.

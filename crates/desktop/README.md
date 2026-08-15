# HD2 Mod Tools — Desktop

The desktop frontend mirrors `crates/web_ui`, but performs patch inspection,
migration, and Unit repatching in native Rust instead of WebAssembly.

## Architecture

- Shared Web UI components, theme, mapping rules, and translations are imported
  from `crates/web_ui/src`.
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
```

From the workspace root:

```powershell
cargo test -p hd2_migrator_desktop
cargo clippy -p hd2_migrator_desktop --all-targets --no-deps
```

The release executable is written to
`target/release/hd2_migrator_desktop.exe`.

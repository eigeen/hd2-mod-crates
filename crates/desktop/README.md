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
```

`bun run smoke` launches the packaged release executable, verifies the rendered
WebView, native IPC against the real-data fixture, and single-instance behavior,
then closes the process it started. Run `bun run desktop build` first; a plain
`cargo build --release` keeps Tauri's development URL and is not a packaged app.

From the workspace root:

```powershell
cargo test -p hd2_migrator_desktop
cargo clippy -p hd2_migrator_desktop --all-targets --no-deps
```

The release executable is written to
`target/release/hd2_migrator_desktop.exe`. The Windows installer is written to
`target/release/bundle/nsis/HD2 Mod Tools Desktop_<version>_x64-setup.exe`.

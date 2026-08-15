const workspaceRoot = new URL("../../../", import.meta.url).pathname.replace(/^\/(\w:)/, "$1");
const executablePath = `${workspaceRoot}target/release/hd2_migrator_desktop.exe`;

const workspaceManifest = await Bun.file(`${workspaceRoot}Cargo.toml`).text();
const desktopManifest = await Bun.file(`${workspaceRoot}crates/desktop/src-tauri/Cargo.toml`).text();
const packageManifest = await Bun.file(`${workspaceRoot}crates/desktop/package.json`).json() as {
  version?: string;
};
const tauriConfig = await Bun.file(`${workspaceRoot}crates/desktop/src-tauri/tauri.conf.json`).json() as {
  version?: string;
};

const version = workspaceManifest.match(/\[workspace\.package\][\s\S]*?\nversion\s*=\s*"([^"]+)"/)?.[1];
if (!version) throw new Error("Workspace package version is missing");
if (!/\nversion\.workspace\s*=\s*true/.test(desktopManifest)) {
  throw new Error("Desktop Cargo package must inherit workspace.package.version");
}
if (tauriConfig.version !== undefined) {
  throw new Error("Tauri config must inherit the Desktop Cargo package version");
}
if (packageManifest.version !== version) {
  throw new Error(`Desktop package.json version ${packageManifest.version} does not match ${version}`);
}

const executable = Bun.file(executablePath);
if (!await executable.exists()) throw new Error(`Desktop executable is missing: ${executablePath}`);
const hasher = new Bun.CryptoHasher("sha256");
hasher.update(await executable.arrayBuffer());
const revision = Bun.spawnSync(["git", "rev-parse", "--short=7", "HEAD"], {
  cwd: workspaceRoot,
}).stdout.toString().trim();

console.log(JSON.stringify({
  bytes: executable.size,
  path: executablePath.replaceAll("/", "\\"),
  revision,
  sha256: hasher.digest("hex"),
  version,
}, null, 2));

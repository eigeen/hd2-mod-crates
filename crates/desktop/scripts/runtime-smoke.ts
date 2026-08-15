const DEBUG_PORT = 19222;
const START_TIMEOUT_MS = 15_000;
const workspaceRoot = new URL("../../../", import.meta.url).pathname.replace(/^\/(\w:)/, "$1");
const executable = `${workspaceRoot}target/release/hd2_migrator_desktop.exe`;
const fixture = process.env.HD2_DESKTOP_TEST_PATCH
  ?? `${workspaceRoot}test_files/SSD'S Stylized Dune 15086 0.1 2026-08-13T05-50Z IzUPRhJHc`;
const dataDir = process.env.HD2_DESKTOP_TEST_DATA
  ?? "C:/Program Files (x86)/Steam/steamapps/common/Helldivers 2/data";

interface CdpResponse {
  error?: { message: string };
  id?: number;
  result?: unknown;
}

class CdpClient {
  private nextId = 1;
  private pending = new Map<number, { reject: (error: Error) => void; resolve: (value: unknown) => void }>();

  constructor(private socket: WebSocket) {
    socket.onmessage = (event) => this.receive(JSON.parse(String(event.data)) as CdpResponse);
  }

  call(method: string, params: Record<string, unknown> = {}): Promise<unknown> {
    const id = this.nextId++;
    this.socket.send(JSON.stringify({ id, method, params }));
    return new Promise((resolve, reject) => this.pending.set(id, { reject, resolve }));
  }

  close(): void {
    this.socket.close();
  }

  private receive(message: CdpResponse): void {
    if (!message.id) return;
    const request = this.pending.get(message.id);
    if (!request) return;
    this.pending.delete(message.id);
    if (message.error) request.reject(new Error(message.error.message));
    else request.resolve(message.result);
  }
}

const app = Bun.spawn([executable], {
  env: {
    ...process.env,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${DEBUG_PORT}`,
  },
  stderr: "pipe",
  stdout: "pipe",
});

let client: CdpClient | null = null;
try {
  const target = await waitForPageTarget();
  const socket = new WebSocket(target.webSocketDebuggerUrl);
  await waitForSocket(socket);
  client = new CdpClient(socket);
  await waitForApplicationDocument(client);
  const result = await evaluateSmokeChecks(client);
  assertSmokeResult(result);
  await verifySingleInstance();
  console.log(JSON.stringify(result, null, 2));
} finally {
  client?.close();
  app.kill();
  await app.exited;
}

async function waitForPageTarget(): Promise<{ webSocketDebuggerUrl: string }> {
  const deadline = Date.now() + START_TIMEOUT_MS;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`http://127.0.0.1:${DEBUG_PORT}/json`);
      const targets = await response.json() as Array<{
        type: string;
        url: string;
        webSocketDebuggerUrl: string;
      }>;
      const page = targets.find((target) => target.url.includes("tauri.localhost"))
        ?? targets.find((target) => target.url !== "about:blank");
      if (page) return page;
    } catch {
      // WebView2 has not opened its debugging endpoint yet.
    }
    await Bun.sleep(100);
  }
  throw new Error("Desktop WebView did not expose a page before the smoke-test timeout");
}

function waitForSocket(socket: WebSocket): Promise<void> {
  return new Promise((resolve, reject) => {
    socket.onopen = () => resolve();
    socket.onerror = () => reject(new Error("Could not connect to the Desktop WebView"));
  });
}

async function waitForApplicationDocument(client: CdpClient): Promise<void> {
  const deadline = Date.now() + START_TIMEOUT_MS;
  while (Date.now() < deadline) {
    const response = await client.call("Runtime.evaluate", {
      expression: "document.title",
      returnByValue: true,
    }) as { result?: { value?: unknown } };
    if (response.result?.value === "HD2 Mod Tools Desktop") return;
    await Bun.sleep(100);
  }
  const targets = await fetch(`http://127.0.0.1:${DEBUG_PORT}/json`).then((response) => response.text());
  throw new Error(`Desktop WebView did not render the application document: ${targets}`);
}

async function evaluateSmokeChecks(client: CdpClient): Promise<unknown> {
  const expression = `
    (async () => {
      const invoke = window.__TAURI_INTERNALS__?.invoke;
      if (!invoke) throw new Error("Tauri IPC bridge is unavailable: " + JSON.stringify({
        href: location.href,
        tauriKeys: Object.getOwnPropertyNames(window).filter((key) => key.includes("TAURI"))
      }));
      const equipment = await invoke("load_equipment_options");
      await invoke("validate_game_data_dir", { path: ${JSON.stringify(dataDir)} });
      const inspection = await invoke("inspect_patch", {
        request: { paths: [${JSON.stringify(fixture)}], dataDir: ${JSON.stringify(dataDir)} }
      });
      return {
        bodyHasAppTitle: document.body.innerText.includes("HD2 Mod"),
        equipmentCount: equipment.length,
        patchName: inspection.patch.name,
        sourceCount: inspection.inspection.sources.length,
        title: document.title
      };
    })()
  `;
  const response = await client.call("Runtime.evaluate", {
    awaitPromise: true,
    expression,
    returnByValue: true,
  }) as { exceptionDetails?: { text: string }; result?: { value?: unknown } };
  if (response.exceptionDetails) throw new Error(response.exceptionDetails.text);
  return response.result?.value;
}

function assertSmokeResult(value: unknown): asserts value is Record<string, unknown> {
  const result = value as Record<string, unknown> | null;
  if (!result || result.title !== "HD2 Mod Tools Desktop") throw new Error("Unexpected window document");
  if (!result.bodyHasAppTitle) throw new Error("Application UI did not render");
  if (typeof result.equipmentCount !== "number" || result.equipmentCount === 0) throw new Error("Equipment IPC returned no data");
  if (typeof result.sourceCount !== "number" || result.sourceCount === 0) throw new Error("Fixture inspection found no sources");
}

async function verifySingleInstance(): Promise<void> {
  const second = Bun.spawn([executable], { stderr: "pipe", stdout: "pipe" });
  const exitCode = await Promise.race([second.exited, Bun.sleep(5_000).then(() => null)]);
  if (exitCode === null) {
    second.kill();
    throw new Error("A second Desktop process remained running");
  }
}

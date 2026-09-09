import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { StrictMode, act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { I18nProvider, useI18n, type LanguageCode } from "@hd2-mod-tools/migrator-ui";
import { MigratorApp } from "../src/MigratorApp";
import type { MigratorAppProps, MigratorPlatform, SetupPanelsProps } from "../src/platform";
import { installDomTestHarness } from "./domTestHarness";

let cleanupDom: (() => void) | undefined;
let restoreGlobals: (() => void) | undefined;
let root: Root | undefined;
let container: HTMLElement;

beforeEach(() => {
  const harness = installDomTestHarness();
  container = harness.container;
  cleanupDom = harness.cleanup;
  restoreGlobals = installAppGlobals();
});

afterEach(async () => {
  try {
    if (root) await act(() => root?.unmount());
  } finally {
    root = undefined;
    restoreGlobals?.();
    cleanupDom?.();
    restoreGlobals = undefined;
    cleanupDom = undefined;
  }
});

describe("MigratorApp initialization effects", () => {
  test("changing extras or language does not reload stable platform data", async () => {
    const calls = { loadEquipmentOptions: 0, restoreGameData: 0 };
    const platform = createPlatform(calls);
    let setLanguage: ((language: LanguageCode) => void) | undefined;
    const baseProps = createAppProps(platform);

    root = createRoot(container);
    await renderApp(root, baseProps, (setter) => { setLanguage = setter; });
    const initializedCalls = { ...calls };
    expect(initializedCalls.loadEquipmentOptions).toBeGreaterThan(0);
    expect(initializedCalls.restoreGameData).toBeGreaterThan(0);

    await renderApp(root, {
      ...baseProps,
      platform: { ...platform },
      extras: { footer: <footer>Changed footer</footer> },
    }, (setter) => { setLanguage = setter; });
    expect(calls).toEqual(initializedCalls);

    await act(() => setLanguage!("zh-CN"));
    expect(document.documentElement.lang).toBe("zh-CN");
    expect(calls).toEqual(initializedCalls);
  });

  test("a delayed directory restore cannot replace a user's selection", async () => {
    const platform = createPlatform({ loadEquipmentOptions: 0, restoreGameData: 0 });
    let finishRestore!: (selection: SetupPanelsProps<string, string>["gameData"]) => void;
    const restored = new Promise<SetupPanelsProps<string, string>["gameData"]>((resolve) => {
      finishRestore = resolve;
    });
    platform.restoreGameData = () => restored;
    let setupProps!: SetupPanelsProps<string, string>;
    function SetupProbe(props: SetupPanelsProps<string, string>) {
      setupProps = props;
      return <span data-game-directory>{props.gameData?.label ?? "empty"}</span>;
    }
    root = createRoot(container);
    await renderApp(root, {
      ...createAppProps(platform),
      SetupPanels: SetupProbe,
      toolMode: "repatch",
    }, () => undefined);

    await act(() => setupProps.onGameDataChange({
      key: "chosen", label: "Chosen directory", ready: true, source: "chosen",
    }));
    await act(async () => finishRestore({
      key: "remembered", label: "Remembered directory", ready: true, source: "remembered",
    }));

    expect(container.querySelector("[data-game-directory]")?.textContent).toBe("Chosen directory");
  });
});

async function renderApp(
  appRoot: Root,
  props: MigratorAppProps<string, string>,
  captureLanguage: (setter: (language: LanguageCode) => void) => void,
) {
  await act(async () => {
    appRoot.render(
      <StrictMode>
        <I18nProvider>
          <LanguageSetterProbe capture={captureLanguage} />
          <MigratorApp {...props} />
        </I18nProvider>
      </StrictMode>,
    );
  });
}

function LanguageSetterProbe({ capture }: { capture: (setter: (language: LanguageCode) => void) => void }) {
  capture(useI18n().setLanguage);
  return null;
}

function createAppProps(platform: MigratorPlatform<string, string>): MigratorAppProps<string, string> {
  return {
    HeadMergeTool: EmptyTool,
    MergeTool: EmptyTool,
    SetupPanels: EmptySetupPanels,
    layout: "web",
    onToolModeChange: () => undefined,
    platform,
    toolMode: "merge",
  };
}

function createPlatform(calls: { loadEquipmentOptions: number; restoreGameData: number }): MigratorPlatform<string, string> {
  return {
    copyDiagnostic: async () => undefined,
    inspectPatch: async () => { throw new Error("inspectPatch should not run in this test"); },
    loadEquipmentOptions: () => {
      calls.loadEquipmentOptions += 1;
      return new Promise(() => undefined);
    },
    previewMappings: async () => { throw new Error("previewMappings should not run in this test"); },
    restoreGameData: () => {
      calls.restoreGameData += 1;
      return new Promise(() => undefined);
    },
    runMigration: async () => null,
    runRepatch: async () => null,
  };
}

function EmptyTool() {
  return <div>Tool</div>;
}

function EmptySetupPanels(_props: SetupPanelsProps<string, string>) {
  return null;
}

function installAppGlobals() {
  const replacements = {
    __GIT_HASH__: "test-revision",
    fetch: (() => new Promise<Response>(() => undefined)) as typeof fetch,
  };
  const previousDescriptors = new Map<string, PropertyDescriptor | undefined>();
  for (const [name, value] of Object.entries(replacements)) {
    previousDescriptors.set(name, Object.getOwnPropertyDescriptor(globalThis, name));
    Object.defineProperty(globalThis, name, { configurable: true, writable: true, value });
  }
  return () => {
    for (const name of Object.keys(replacements)) {
      const descriptor = previousDescriptors.get(name);
      if (descriptor) Object.defineProperty(globalThis, name, descriptor);
      else Reflect.deleteProperty(globalThis, name);
    }
  };
}

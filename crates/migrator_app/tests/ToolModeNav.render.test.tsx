import { afterEach, beforeEach, expect, spyOn, test } from "bun:test";
import { StrictMode, act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { ToolModeNav } from "@hd2-mod-tools/migrator-ui";
import { installDomTestHarness } from "./domTestHarness";

let harness: ReturnType<typeof installDomTestHarness>;
let root: Root;
let restoreMeasurement: () => void;
let controlLeft: number;
let controlWidth: number;

beforeEach(() => {
  harness = installDomTestHarness();
  root = createRoot(harness.container);
  controlLeft = 180;
  controlWidth = 120;
  const measurement = spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const isControl = this.hasAttribute("data-tool-nav-active");
    const left = isControl ? controlLeft : 20;
    const width = isControl ? controlWidth : 600;
    return { left, width, top: 0, bottom: 48, right: left + width, height: 48, x: left, y: 0, toJSON() {} };
  });
  restoreMeasurement = () => measurement.mockRestore();
});

afterEach(async () => {
  await act(() => root.unmount());
  restoreMeasurement();
  harness.cleanup();
});

test.each([false, true])("positions the underline on initial mount (StrictMode=%s)", async (strict) => {
  const nav = <ToolModeNav><button data-tool-nav-active="true">Update Mod Version</button></ToolModeNav>;
  await act(() => root.render(strict ? <StrictMode>{nav}</StrictMode> : nav));
  expect(indicator().style.transform).toBe("translateX(160px)");
  expect(indicator().style.width).toBe("120px");
});

test("keeps the same indicator element when switching to advanced tools", async () => {
  await renderNavigation(false);
  const original = indicator();
  controlLeft = 330;
  controlWidth = 150;
  await renderNavigation(true);
  expect(indicator()).toBe(original);
  expect(original.style.transform).toBe("translateX(310px)");
  expect(original.style.width).toBe("150px");
});

test("remeasures controls when layout changes and disconnects on unmount", async () => {
  let notifyResize = () => {};
  const observed: Element[] = [];
  let disconnected = false;
  const OriginalObserver = window.ResizeObserver;
  window.ResizeObserver = class {
    constructor(callback: () => void) { notifyResize = callback; }
    observe(element: Element) { observed.push(element); }
    unobserve() {}
    disconnect() { disconnected = true; }
  } as unknown as typeof ResizeObserver;
  try {
    await renderNavigation(false);
    expect(observed).toHaveLength(3);
    controlLeft = 210;
    controlWidth = 140;
    await act(() => notifyResize());
    expect(indicator().style.transform).toBe("translateX(190px)");
    expect(indicator().style.width).toBe("140px");
    await act(() => root.render(null));
    expect(disconnected).toBe(true);
  } finally {
    window.ResizeObserver = OriginalObserver;
  }
});

function indicator() {
  const element = harness.container.querySelector<HTMLElement>(".toolModeIndicator");
  if (!element) throw new Error("The selected tool must have an underline on first render");
  return element;
}

async function renderNavigation(advanced: boolean) {
  await act(() => root.render(
    <ToolModeNav>
      <button data-tool-nav-active={!advanced}>Update Mod Version</button>
      <button data-tool-nav-active={advanced}>Merge Mods</button>
    </ToolModeNav>,
  ));
}

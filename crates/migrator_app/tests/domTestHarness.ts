import { Window } from "happy-dom";

const DOM_GLOBAL_NAMES = [
  "window",
  "document",
  "navigator",
  "HTMLElement",
  "Element",
  "Node",
  "Text",
  "Event",
  "CustomEvent",
  "MutationObserver",
  "getComputedStyle",
  "requestAnimationFrame",
  "cancelAnimationFrame",
  "IS_REACT_ACT_ENVIRONMENT",
] as const;

type GlobalName = (typeof DOM_GLOBAL_NAMES)[number];

/** Install an isolated DOM and restore every replaced global after the test. */
export function installDomTestHarness() {
  const window = new Window({ url: "http://localhost/" });
  const previousDescriptors = new Map<GlobalName, PropertyDescriptor | undefined>();
  const globalValues: Record<GlobalName, unknown> = {
    window,
    document: window.document,
    navigator: window.navigator,
    HTMLElement: window.HTMLElement,
    Element: window.Element,
    Node: window.Node,
    Text: window.Text,
    Event: window.Event,
    CustomEvent: window.CustomEvent,
    MutationObserver: window.MutationObserver,
    getComputedStyle: window.getComputedStyle.bind(window),
    requestAnimationFrame: window.requestAnimationFrame.bind(window),
    cancelAnimationFrame: window.cancelAnimationFrame.bind(window),
    IS_REACT_ACT_ENVIRONMENT: true,
  };

  for (const name of DOM_GLOBAL_NAMES) {
    previousDescriptors.set(name, Object.getOwnPropertyDescriptor(globalThis, name));
    Object.defineProperty(globalThis, name, {
      configurable: true,
      writable: true,
      value: globalValues[name],
    });
  }

  const container = window.document.createElement("div") as unknown as HTMLElement;
  window.document.body.append(container);

  return {
    container,
    cleanup() {
      container.remove();
      window.close();
      for (const name of DOM_GLOBAL_NAMES) {
        const descriptor = previousDescriptors.get(name);
        if (descriptor) Object.defineProperty(globalThis, name, descriptor);
        else Reflect.deleteProperty(globalThis, name);
      }
    },
  };
}

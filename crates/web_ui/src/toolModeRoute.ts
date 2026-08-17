import { useCallback, useEffect, useState } from "react";

const TOOL_MODES = ["migrate", "repatch", "merge"] as const;

export type ToolMode = (typeof TOOL_MODES)[number];

/** Resolves both canonical `#/mode` hashes and legacy `#mode` hashes. */
export function toolModeFromHash(hash: string): ToolMode {
  const route = hash.replace(/^#\/?/, "").replace(/\/+$/, "");
  return isToolMode(route) ? route : "migrate";
}

export function hashForToolMode(mode: ToolMode): string {
  return `#/${mode}`;
}

/** Keeps the selected tool synchronized with browser hash history. */
export function useToolModeRoute() {
  const [toolMode, setToolMode] = useState<ToolMode>(
    () => toolModeFromHash(window.location.hash),
  );

  useEffect(() => {
    const syncFromHash = () => {
      const nextMode = toolModeFromHash(window.location.hash);
      setToolMode(nextMode);
      normalizeHash(nextMode);
    };
    syncFromHash();
    window.addEventListener("hashchange", syncFromHash);
    return () => window.removeEventListener("hashchange", syncFromHash);
  }, []);

  const navigateToToolMode = useCallback((nextMode: ToolMode) => {
    setToolMode(nextMode);
    const nextHash = hashForToolMode(nextMode);
    if (window.location.hash !== nextHash) window.location.hash = nextHash;
  }, []);

  return [toolMode, navigateToToolMode] as const;
}

function isToolMode(value: string): value is ToolMode {
  return TOOL_MODES.some((mode) => mode === value);
}

function normalizeHash(mode: ToolMode) {
  const canonicalHash = hashForToolMode(mode);
  if (window.location.hash === canonicalHash) return;
  window.history.replaceState(window.history.state, "", canonicalHash);
}

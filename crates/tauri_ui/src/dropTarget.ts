import type { PhysicalPosition } from "@tauri-apps/api/dpi";
import type { PathField } from "./types";

const dropFieldAttribute = "data-drop-field";

/// Resolve a Tauri physical drag position to the path field currently under it.
export function fieldFromPhysicalPosition(position: PhysicalPosition): PathField | null {
  const point = clientPointFromPhysicalPosition(position);
  const element = document.elementFromPoint(point.x, point.y);
  return fieldFromElement(element);
}

function clientPointFromPhysicalPosition(position: PhysicalPosition) {
  const scale = window.devicePixelRatio || 1;
  return {
    x: position.x / scale,
    y: position.y / scale,
  };
}

function fieldFromElement(element: Element | null): PathField | null {
  const target = element?.closest(`[${dropFieldAttribute}]`);
  const value = target?.getAttribute(dropFieldAttribute);
  return isPathField(value) ? value : null;
}

function isPathField(value: string | null | undefined): value is PathField {
  return value === "patchPath" || value === "dataDir" || value === "outDir";
}

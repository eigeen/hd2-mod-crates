import type { PhysicalPosition } from "@tauri-apps/api/dpi";

export type DropZone = "gameData" | "patch";

const DROP_ZONE_ATTRIBUTE = "data-drop-zone";

/** Resolve Tauri's physical pointer coordinates to a desktop import panel. */
export function dropZoneFromPhysicalPosition(position: PhysicalPosition): DropZone | null {
  const scale = window.devicePixelRatio || 1;
  const element = document.elementFromPoint(position.x / scale, position.y / scale);
  const value = element?.closest(`[${DROP_ZONE_ATTRIBUTE}]`)?.getAttribute(DROP_ZONE_ATTRIBUTE);
  return value === "gameData" || value === "patch" ? value : null;
}

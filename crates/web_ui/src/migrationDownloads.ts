import type { TargetOption } from "./types";

export function uniqueOutputFilename(
  sourceName: string,
  targetHash: string,
  targets: TargetOption[],
): string {
  const target = targets.find((candidate) => candidate.hash === targetHash);
  const sourceLabel = sanitizeFilenameSegment(sourceName.replace(/\.zip$/i, ""));
  const targetLabel = sanitizeFilenameSegment(target?.name ?? targetHash);
  return `${sourceLabel}_${targetLabel}.zip`;
}

function sanitizeFilenameSegment(value: string): string {
  const sanitized = value.replace(/[\\/:*?"<>|]/g, "_").trim();
  return sanitized || "target";
}

import { buildMetadataJson, builtinTargetOptions, listTargets } from "./wasmClient";
import { archivesFromGameDirectory, metadataTextFromFile } from "./fileInputs";
import type { MetadataState, TargetOption } from "./types";

const PUBLIC_METADATA_URL = "/assets/web_game_metadata.json";

export async function loadPublicMetadata() {
  const response = await fetch(PUBLIC_METADATA_URL);
  if (!response.ok) {
    throw new Error(`加载内置元数据失败：${response.status} ${response.statusText}`);
  }
  const json = await response.text();
  return metadataState(json, "内置资源");
}

export async function loadMetadataFile(file: File) {
  const json = await metadataTextFromFile(file);
  return metadataState(json, file.name);
}

export async function buildMetadataFromDirectory(targets: TargetOption[]) {
  const archives = await archivesFromGameDirectory(targets);
  const json = await buildMetadataJson("Armor", archives);
  return metadataState(json, "浏览器目录元数据");
}

export async function fallbackTargetOptions(current: MetadataState | null) {
  if (current && current.targetCount > 0) {
    return listTargets(current.json);
  }
  return builtinTargetOptions("Armor");
}

async function metadataState(json: string, source: string) {
  const targets = await listTargets(json);
  return {
    json,
    source,
    targetCount: targets.length,
  } satisfies MetadataState;
}

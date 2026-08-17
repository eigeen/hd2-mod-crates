import type { LanguageCode } from "./i18n";

const UPDATE_INFO_STORAGE_KEY = "hd2-migrator-update-info";
const UPDATE_INFO_STORAGE_VERSION = 1;

export interface UpdateInfoRelease {
  files: Record<LanguageCode, string>;
  id: string;
  releasedAt: string;
  sequence: number;
  titles: Record<LanguageCode, string>;
  version: string;
}

export interface UpdateInfoManifest {
  releases: UpdateInfoRelease[];
  schemaVersion: 1;
}

export interface UpdateInfoStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export function parseUpdateInfoManifest(value: unknown): UpdateInfoManifest {
  if (!isRecord(value) || value.schemaVersion !== 1 || !Array.isArray(value.releases)) {
    throw new Error("Unsupported update information manifest");
  }
  const releases = value.releases.map(parseRelease);
  if (!isStrictlyDescending(releases.map((release) => release.sequence))) {
    throw new Error("Update information releases are not ordered by sequence");
  }
  return { schemaVersion: 1, releases };
}

export function loadHighestSeenSequence(storage: UpdateInfoStorage): number {
  try {
    const value = JSON.parse(storage.getItem(UPDATE_INFO_STORAGE_KEY) ?? "null");
    if (!isRecord(value) || value.schemaVersion !== UPDATE_INFO_STORAGE_VERSION) return 0;
    return Number.isInteger(value.highestSeenSequence) && Number(value.highestSeenSequence) > 0
      ? Number(value.highestSeenSequence)
      : 0;
  } catch {
    return 0;
  }
}

export function storeHighestSeenSequence(storage: UpdateInfoStorage, sequence: number): void {
  try {
    storage.setItem(UPDATE_INFO_STORAGE_KEY, JSON.stringify({
      schemaVersion: UPDATE_INFO_STORAGE_VERSION,
      highestSeenSequence: sequence,
    }));
  } catch {
    // Update information is optional; unavailable storage must not block the app.
  }
}

export function shouldShowLatestRelease(manifest: UpdateInfoManifest, highestSeenSequence: number): boolean {
  return (manifest.releases[0]?.sequence ?? 0) > highestSeenSequence;
}

function parseRelease(value: unknown): UpdateInfoRelease {
  if (!isRecord(value)) throw new Error("Invalid update information release");
  const release: UpdateInfoRelease = {
    files: parseLocalizedStrings(value.files, "files"),
    id: requiredString(value.id, "id"),
    releasedAt: requiredString(value.releasedAt, "releasedAt"),
    sequence: requiredPositiveInteger(value.sequence, "sequence"),
    titles: parseLocalizedStrings(value.titles, "titles"),
    version: requiredString(value.version, "version"),
  };
  return release;
}

function parseLocalizedStrings(value: unknown, name: string): Record<LanguageCode, string> {
  if (!isRecord(value)) throw new Error(`Invalid update information ${name}`);
  return {
    "zh-CN": requiredString(value["zh-CN"], `${name}.zh-CN`),
    en: requiredString(value.en, `${name}.en`),
  };
}

function requiredString(value: unknown, name: string): string {
  if (typeof value !== "string" || !value.trim()) throw new Error(`Invalid update information ${name}`);
  return value;
}

function requiredPositiveInteger(value: unknown, name: string): number {
  if (!Number.isInteger(value) || Number(value) < 1) throw new Error(`Invalid update information ${name}`);
  return Number(value);
}

function isStrictlyDescending(values: number[]): boolean {
  return values.every((value, index) => index === 0 || values[index - 1] > value);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

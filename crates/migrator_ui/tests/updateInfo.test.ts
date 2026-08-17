import { expect, test } from "bun:test";
import {
  loadHighestSeenSequence,
  parseUpdateInfoManifest,
  shouldShowLatestRelease,
  storeHighestSeenSequence,
  type UpdateInfoStorage,
} from "../src/updateInfo";

const manifestValue = {
  schemaVersion: 1,
  releases: [
    {
      files: { "zh-CN": "./new/zh-CN.md", en: "./new/en.md" },
      id: "new",
      releasedAt: "2026-08-17",
      sequence: 2,
      titles: { "zh-CN": "新版", en: "New" },
      version: "0.2.0",
    },
    {
      files: { "zh-CN": "./old/zh-CN.md", en: "./old/en.md" },
      id: "old",
      releasedAt: "2026-07-01",
      sequence: 1,
      titles: { "zh-CN": "旧版", en: "Old" },
      version: "0.1.0",
    },
  ],
};

test("parses ordered update releases and detects unread latest release", () => {
  const manifest = parseUpdateInfoManifest(manifestValue);
  expect(manifest.releases.map((release) => release.id)).toEqual(["new", "old"]);
  expect(shouldShowLatestRelease(manifest, 1)).toBe(true);
  expect(shouldShowLatestRelease(manifest, 2)).toBe(false);
  expect(shouldShowLatestRelease(manifest, 3)).toBe(false);
});

test("rejects unordered update releases", () => {
  const unordered = { ...manifestValue, releases: [...manifestValue.releases].reverse() };
  expect(() => parseUpdateInfoManifest(unordered)).toThrow("not ordered");
});

test("persists versioned highest-seen sequence and tolerates corrupt storage", () => {
  const storage = memoryStorage(null);
  expect(loadHighestSeenSequence(storage)).toBe(0);
  storeHighestSeenSequence(storage, 7);
  expect(loadHighestSeenSequence(storage)).toBe(7);
  storage.value = "not json";
  expect(loadHighestSeenSequence(storage)).toBe(0);
});

interface MutableStorage extends UpdateInfoStorage {
  value: string | null;
}

function memoryStorage(initialValue: string | null): MutableStorage {
  return {
    value: initialValue,
    getItem() {
      return this.value;
    },
    setItem(_key, value) {
      this.value = value;
    },
  };
}

import { expect, test } from "bun:test";
import { mkdtemp, readFile, rm, writeFile, mkdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { buildUpdateInfo } from "../scripts/updateInfoBuilder";

test("builds only the three newest localized releases and copies images", async () => {
  const workspace = await mkdtemp(resolve(tmpdir(), "hd2-update-info-"));
  const sourceDirectory = resolve(workspace, "source");
  const outputDirectory = resolve(workspace, "output");
  try {
    await Promise.all([
      writeRelease(sourceDirectory, "one", 1),
      writeRelease(sourceDirectory, "two", 2),
      writeRelease(sourceDirectory, "three", 3),
      writeRelease(sourceDirectory, "four", 4, true),
    ]);

    const manifest = await buildUpdateInfo({ sourceDirectory, outputDirectory });

    expect(manifest.releases.map((release) => release.id)).toEqual(["four", "three", "two"]);
    expect(await readFile(resolve(outputDirectory, "releases/four/preview.webp"))).toEqual(Uint8Array.from([1, 2, 3]));
    expect(await readFile(resolve(outputDirectory, "releases/four/zh-CN.md"), "utf8")).not.toContain("sequence:");
    expect(await readFile(resolve(outputDirectory, "index.json"), "utf8")).toContain('"schemaVersion": 1');
  } finally {
    await rm(workspace, { force: true, recursive: true });
  }
});

test("rejects missing local images before replacing existing output", async () => {
  const workspace = await mkdtemp(resolve(tmpdir(), "hd2-update-info-invalid-"));
  const sourceDirectory = resolve(workspace, "source");
  const outputDirectory = resolve(workspace, "output");
  try {
    await writeRelease(sourceDirectory, "broken", 1, true, false);
    await expect(buildUpdateInfo({ sourceDirectory, outputDirectory })).rejects.toThrow("Missing update image");
  } finally {
    await rm(workspace, { force: true, recursive: true });
  }
});

async function writeRelease(
  sourceDirectory: string,
  id: string,
  sequence: number,
  referencesImage = false,
  writesImage = true,
): Promise<void> {
  const releaseDirectory = resolve(sourceDirectory, id);
  await mkdir(releaseDirectory, { recursive: true });
  await Promise.all([
    writePage(releaseDirectory, id, sequence, "zh-CN", referencesImage),
    writePage(releaseDirectory, id, sequence, "en", referencesImage),
  ]);
  if (referencesImage && writesImage) await writeFile(resolve(releaseDirectory, "preview.webp"), Uint8Array.from([1, 2, 3]));
}

async function writePage(
  releaseDirectory: string,
  id: string,
  sequence: number,
  locale: "zh-CN" | "en",
  referencesImage: boolean,
): Promise<void> {
  const image = referencesImage ? "\n![Preview](./preview.webp)\n" : "";
  const markdown = `---\nid: ${id}\nsequence: ${sequence}\nversion: "0.${sequence}.0"\nreleasedAt: "2026-08-17"\nlocale: ${locale}\ntitle: ${id} ${locale}\n---\n\n# ${id}\n${image}`;
  await writeFile(resolve(releaseDirectory, `${locale}.md`), markdown, "utf8");
}

import { cp, mkdir, readdir, readFile, rename, rm, stat, writeFile } from "node:fs/promises";
import { dirname, extname, relative, resolve, sep } from "node:path";
import { parse as parseYaml } from "yaml";

export const MAX_UPDATE_RELEASES = 3;
export const UPDATE_LOCALES = ["zh-CN", "en"] as const;

type UpdateLocale = (typeof UPDATE_LOCALES)[number];

interface ReleaseMetadata {
  id: string;
  locale: UpdateLocale;
  releasedAt: string;
  sequence: number;
  title: string;
  version: string;
}

interface ParsedPage {
  body: string;
  filePath: string;
  metadata: ReleaseMetadata;
}

interface ParsedRelease {
  directory: string;
  id: string;
  releasedAt: string;
  sequence: number;
  version: string;
  pages: Record<UpdateLocale, ParsedPage>;
}

export interface UpdateInfoManifestRelease {
  files: Record<UpdateLocale, string>;
  id: string;
  releasedAt: string;
  sequence: number;
  titles: Record<UpdateLocale, string>;
  version: string;
}

export interface UpdateInfoManifest {
  releases: UpdateInfoManifestRelease[];
  schemaVersion: 1;
}

export interface BuildUpdateInfoOptions {
  limit?: number;
  outputDirectory: string;
  sourceDirectory: string;
}

const ALLOWED_IMAGE_EXTENSIONS = new Set([".gif", ".jpeg", ".jpg", ".png", ".webp"]);
const FRONT_MATTER_PATTERN = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?/;
const MARKDOWN_IMAGE_PATTERN = /!\[[^\]]*\]\((?:<([^>]+)>|([^\s)]+))(?:\s+["'][^"']*["'])?\)/g;

/** Build deterministic runtime update resources from authored release directories. */
export async function buildUpdateInfo(options: BuildUpdateInfoOptions): Promise<UpdateInfoManifest> {
  const releases = await loadReleases(options.sourceDirectory);
  const selected = selectRecentReleases(releases, options.limit ?? MAX_UPDATE_RELEASES);
  await validateReleaseImages(selected);
  const manifest = createManifest(selected);
  await writeGeneratedResources(selected, manifest, options.outputDirectory);
  return manifest;
}

async function loadReleases(sourceDirectory: string): Promise<ParsedRelease[]> {
  const entries = await readdir(sourceDirectory, { withFileTypes: true });
  const directories = entries.filter((entry) => entry.isDirectory()).map((entry) => entry.name).sort();
  const releases = await Promise.all(directories.map((name) => loadRelease(resolve(sourceDirectory, name), name)));
  validateUniqueReleaseFields(releases);
  return releases;
}

async function loadRelease(directory: string, directoryName: string): Promise<ParsedRelease> {
  const pages = await Promise.all(UPDATE_LOCALES.map((locale) => loadPage(directory, locale)));
  const localizedPages = Object.fromEntries(pages.map((page) => [page.metadata.locale, page])) as Record<UpdateLocale, ParsedPage>;
  const reference = localizedPages["zh-CN"].metadata;
  validateReleaseMetadata(directoryName, reference, localizedPages.en.metadata);
  return {
    directory,
    id: reference.id,
    releasedAt: reference.releasedAt,
    sequence: reference.sequence,
    version: reference.version,
    pages: localizedPages,
  };
}

async function loadPage(directory: string, locale: UpdateLocale): Promise<ParsedPage> {
  const filePath = resolve(directory, `${locale}.md`);
  const source = await readFile(filePath, "utf8").catch((error: unknown) => {
    throw new Error(`Cannot read UTF-8 update page ${filePath}: ${errorMessage(error)}`);
  });
  const match = FRONT_MATTER_PATTERN.exec(source);
  if (!match) throw new Error(`Missing YAML front matter in ${filePath}`);
  const metadata = parseMetadata(parseYaml(match[1]), filePath, locale);
  return { body: source.slice(match[0].length).trimStart(), filePath, metadata };
}

function parseMetadata(value: unknown, filePath: string, expectedLocale: UpdateLocale): ReleaseMetadata {
  if (!isRecord(value)) throw new Error(`Front matter must be an object in ${filePath}`);
  const metadata = {
    id: requiredString(value, "id", filePath),
    locale: requiredLocale(value, filePath),
    releasedAt: requiredString(value, "releasedAt", filePath),
    sequence: requiredPositiveInteger(value, "sequence", filePath),
    title: requiredString(value, "title", filePath),
    version: requiredString(value, "version", filePath),
  };
  if (metadata.locale !== expectedLocale) {
    throw new Error(`Locale ${metadata.locale} does not match ${expectedLocale}.md in ${filePath}`);
  }
  if (!isIsoCalendarDate(metadata.releasedAt)) {
    throw new Error(`releasedAt must use YYYY-MM-DD in ${filePath}`);
  }
  return metadata;
}

function validateReleaseMetadata(directoryName: string, reference: ReleaseMetadata, localized: ReleaseMetadata): void {
  if (reference.id !== directoryName) throw new Error(`Release directory ${directoryName} must match front matter id ${reference.id}`);
  for (const field of ["id", "releasedAt", "sequence", "version"] as const) {
    if (reference[field] !== localized[field]) throw new Error(`Release ${reference.id} has inconsistent ${field} metadata across locales`);
  }
}

function validateUniqueReleaseFields(releases: ParsedRelease[]): void {
  const ids = new Set<string>();
  const sequences = new Set<number>();
  for (const release of releases) {
    if (ids.has(release.id)) throw new Error(`Duplicate update release id: ${release.id}`);
    if (sequences.has(release.sequence)) throw new Error(`Duplicate update release sequence: ${release.sequence}`);
    ids.add(release.id);
    sequences.add(release.sequence);
  }
}

function selectRecentReleases(releases: ParsedRelease[], limit: number): ParsedRelease[] {
  if (!Number.isInteger(limit) || limit < 1) throw new Error("Update release limit must be a positive integer");
  return [...releases].sort((left, right) => right.sequence - left.sequence).slice(0, limit);
}

async function validateReleaseImages(releases: ParsedRelease[]): Promise<void> {
  for (const release of releases) {
    for (const page of Object.values(release.pages)) await validateMarkdownImages(page, release.directory);
  }
}

async function validateMarkdownImages(page: ParsedPage, releaseDirectory: string): Promise<void> {
  for (const target of markdownImageTargets(page.body)) {
    const imagePath = resolveLocalImagePath(target, releaseDirectory, page.filePath);
    const imageStat = await stat(imagePath).catch(() => null);
    if (!imageStat?.isFile()) throw new Error(`Missing update image ${target} referenced by ${page.filePath}`);
  }
}

function markdownImageTargets(markdown: string): string[] {
  return [...markdown.matchAll(MARKDOWN_IMAGE_PATTERN)].map((match) => match[1] ?? match[2]);
}

function resolveLocalImagePath(target: string, releaseDirectory: string, pagePath: string): string {
  if (/^(?:[a-z]+:|\/)/i.test(target)) throw new Error(`Update images must be local relative files in ${pagePath}: ${target}`);
  const decodedTarget = decodeURIComponent(target.split(/[?#]/, 1)[0]);
  const resolvedPath = resolve(dirname(pagePath), decodedTarget);
  const relativePath = relative(releaseDirectory, resolvedPath);
  if (relativePath.startsWith(`..${sep}`) || relativePath === "..") throw new Error(`Update image escapes its release directory in ${pagePath}: ${target}`);
  if (!ALLOWED_IMAGE_EXTENSIONS.has(extname(resolvedPath).toLowerCase())) throw new Error(`Unsupported update image type in ${pagePath}: ${target}`);
  return resolvedPath;
}

function createManifest(releases: ParsedRelease[]): UpdateInfoManifest {
  return {
    schemaVersion: 1,
    releases: releases.map((release) => ({
      files: Object.fromEntries(UPDATE_LOCALES.map((locale) => [locale, `./releases/${release.id}/${locale}.md`])) as Record<UpdateLocale, string>,
      id: release.id,
      releasedAt: release.releasedAt,
      sequence: release.sequence,
      titles: Object.fromEntries(UPDATE_LOCALES.map((locale) => [locale, release.pages[locale].metadata.title])) as Record<UpdateLocale, string>,
      version: release.version,
    })),
  };
}

async function writeGeneratedResources(releases: ParsedRelease[], manifest: UpdateInfoManifest, outputDirectory: string): Promise<void> {
  const temporaryDirectory = `${outputDirectory}.tmp-${process.pid}`;
  await rm(temporaryDirectory, { force: true, recursive: true });
  await mkdir(temporaryDirectory, { recursive: true });
  try {
    await Promise.all(releases.map((release) => writeRelease(release, temporaryDirectory)));
    await writeFile(resolve(temporaryDirectory, "index.json"), `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
    await rm(outputDirectory, { force: true, recursive: true });
    await rename(temporaryDirectory, outputDirectory);
  } catch (error) {
    await rm(temporaryDirectory, { force: true, recursive: true });
    throw error;
  }
}

async function writeRelease(release: ParsedRelease, outputDirectory: string): Promise<void> {
  const targetDirectory = resolve(outputDirectory, "releases", release.id);
  await mkdir(targetDirectory, { recursive: true });
  await copyReleaseImages(release.directory, targetDirectory);
  await Promise.all(UPDATE_LOCALES.map((locale) => (
    writeFile(resolve(targetDirectory, `${locale}.md`), release.pages[locale].body, "utf8")
  )));
}

async function copyReleaseImages(sourceDirectory: string, targetDirectory: string): Promise<void> {
  const entries = await readdir(sourceDirectory, { withFileTypes: true, recursive: true });
  for (const entry of entries) {
    if (!entry.isFile() || !ALLOWED_IMAGE_EXTENSIONS.has(extname(entry.name).toLowerCase())) continue;
    const sourcePath = resolve(entry.parentPath, entry.name);
    const targetPath = resolve(targetDirectory, relative(sourceDirectory, sourcePath));
    await mkdir(dirname(targetPath), { recursive: true });
    await cp(sourcePath, targetPath);
  }
}

function requiredString(value: Record<string, unknown>, key: string, filePath: string): string {
  const field = value[key];
  if (typeof field !== "string" || !field.trim()) throw new Error(`Front matter field ${key} must be a non-empty string in ${filePath}`);
  return field.trim();
}

function requiredPositiveInteger(value: Record<string, unknown>, key: string, filePath: string): number {
  const field = value[key];
  if (!Number.isInteger(field) || Number(field) < 1) throw new Error(`Front matter field ${key} must be a positive integer in ${filePath}`);
  return Number(field);
}

function requiredLocale(value: Record<string, unknown>, filePath: string): UpdateLocale {
  const locale = requiredString(value, "locale", filePath);
  if (locale !== "zh-CN" && locale !== "en") throw new Error(`Unsupported locale ${locale} in ${filePath}`);
  return locale;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isIsoCalendarDate(value: string): boolean {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return false;
  const [year, month, day] = match.slice(1).map(Number);
  const date = new Date(Date.UTC(year, month - 1, day));
  return date.getUTCFullYear() === year && date.getUTCMonth() === month - 1 && date.getUTCDate() === day;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

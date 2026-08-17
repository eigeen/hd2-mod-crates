import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useI18n, type LanguageCode } from "./i18n";
import {
  loadHighestSeenSequence,
  parseUpdateInfoManifest,
  shouldShowLatestRelease,
  storeHighestSeenSequence,
  type UpdateInfoManifest,
  type UpdateInfoRelease,
} from "./updateInfo";

interface LoadedUpdatePage {
  markdown: string;
  sourceUrl: string;
}

export interface UpdateInfoController {
  available: boolean;
  close: () => void;
  currentPage: LoadedUpdatePage | null;
  currentRelease: UpdateInfoRelease | null;
  error: boolean;
  goToPage: (index: number) => void;
  isOpen: boolean;
  loading: boolean;
  navigationDirection: "newer" | "older" | null;
  openLatest: () => void;
  pageIndex: number;
  releases: UpdateInfoRelease[];
}

const MANIFEST_PATH = "update-info/index.json";

export function useUpdateInfo(): UpdateInfoController {
  const { language } = useI18n();
  const [manifest, setManifest] = useState<UpdateInfoManifest | null>(null);
  const [isOpen, setIsOpen] = useState(false);
  const [pageIndex, setPageIndex] = useState(0);
  const [currentPage, setCurrentPage] = useState<LoadedUpdatePage | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(false);
  const [navigationDirection, setNavigationDirection] = useState<"newer" | "older" | null>(null);
  const pageCache = useRef(new Map<string, LoadedUpdatePage>());
  const manifestUrl = useMemo(resolveManifestUrl, []);
  const releases = manifest?.releases ?? [];
  const currentRelease = releases[pageIndex] ?? null;

  useEffect(() => loadManifest(manifestUrl, setManifest, setIsOpen), [manifestUrl]);
  useEffect(() => {
    if (!isOpen || !currentRelease) return;
    return loadVisiblePage(currentRelease, language, manifestUrl, pageCache.current, {
      setCurrentPage,
      setError,
      setLoading,
    });
  }, [currentRelease, isOpen, language, manifestUrl]);
  useEffect(() => {
    if (!isOpen || !manifest) return;
    preloadAdjacentPages(manifest, pageIndex, language, manifestUrl, pageCache.current);
  }, [isOpen, language, manifest, manifestUrl, pageIndex]);

  const close = useCallback(() => {
    const latestSequence = manifest?.releases[0]?.sequence;
    if (latestSequence) storeHighestSeenSequence(window.localStorage, latestSequence);
    setIsOpen(false);
  }, [manifest]);
  const openLatest = useCallback(() => {
    if (!manifest?.releases.length) return;
    setPageIndex(0);
    setNavigationDirection(null);
    setIsOpen(true);
  }, [manifest]);
  const goToPage = useCallback((index: number) => {
    if (!manifest) return;
    const nextIndex = Math.max(0, Math.min(index, manifest.releases.length - 1));
    if (nextIndex === pageIndex) return;
    setNavigationDirection(nextIndex > pageIndex ? "older" : "newer");
    setPageIndex(nextIndex);
  }, [manifest, pageIndex]);

  return {
    available: releases.length > 0,
    close,
    currentPage,
    currentRelease,
    error,
    goToPage,
    isOpen,
    loading,
    navigationDirection,
    openLatest,
    pageIndex,
    releases,
  };
}

interface PageStateSetters {
  setCurrentPage: (page: LoadedUpdatePage | null) => void;
  setError: (error: boolean) => void;
  setLoading: (loading: boolean) => void;
}

function loadManifest(
  manifestUrl: string,
  setManifest: (manifest: UpdateInfoManifest) => void,
  setIsOpen: (open: boolean) => void,
): () => void {
  const controller = new AbortController();
  void fetch(manifestUrl, { cache: "no-cache", signal: controller.signal })
    .then((response) => response.ok ? response.json() : Promise.reject(new Error(`HTTP ${response.status}`)))
    .then(parseUpdateInfoManifest)
    .then((manifest) => {
      setManifest(manifest);
      setIsOpen(shouldShowLatestRelease(manifest, loadHighestSeenSequence(window.localStorage)));
    })
    .catch((error: unknown) => {
      if (!controller.signal.aborted) console.warn("[update-info] Could not load manifest", error);
    });
  return () => controller.abort();
}

function loadVisiblePage(
  release: UpdateInfoRelease,
  language: LanguageCode,
  manifestUrl: string,
  cache: Map<string, LoadedUpdatePage>,
  setters: PageStateSetters,
): () => void {
  const key = pageKey(release, language);
  const cached = cache.get(key);
  if (cached) {
    setters.setCurrentPage(cached);
    setters.setError(false);
    setters.setLoading(false);
    return () => undefined;
  }
  setters.setCurrentPage(null);
  setters.setError(false);
  setters.setLoading(true);
  const controller = new AbortController();
  void fetchUpdatePage(release, language, manifestUrl, controller.signal).then((page) => {
    cache.set(key, page);
    setters.setCurrentPage(page);
  }).catch((error: unknown) => {
    if (!controller.signal.aborted) {
      console.warn("[update-info] Could not load page", error);
      setters.setError(true);
    }
  }).finally(() => {
    if (!controller.signal.aborted) setters.setLoading(false);
  });
  return () => controller.abort();
}

function preloadAdjacentPages(
  manifest: UpdateInfoManifest,
  pageIndex: number,
  language: LanguageCode,
  manifestUrl: string,
  cache: Map<string, LoadedUpdatePage>,
): void {
  for (const index of [pageIndex - 1, pageIndex + 1]) {
    const release = manifest.releases[index];
    if (!release || cache.has(pageKey(release, language))) continue;
    void fetchUpdatePage(release, language, manifestUrl).then((page) => {
      cache.set(pageKey(release, language), page);
    }).catch(() => undefined);
  }
}

async function fetchUpdatePage(
  release: UpdateInfoRelease,
  language: LanguageCode,
  manifestUrl: string,
  signal?: AbortSignal,
): Promise<LoadedUpdatePage> {
  const sourceUrl = new URL(release.files[language], manifestUrl).href;
  const response = await fetch(sourceUrl, { cache: "no-cache", signal });
  if (!response.ok) throw new Error(`HTTP ${response.status} loading ${sourceUrl}`);
  return { markdown: await response.text(), sourceUrl };
}

function resolveManifestUrl(): string {
  return new URL(MANIFEST_PATH, new URL("/", window.location.href)).href;
}

function pageKey(release: UpdateInfoRelease, language: LanguageCode): string {
  return `${release.id}:${language}`;
}

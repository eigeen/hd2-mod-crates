export const DESKTOP_RELEASE_URL = "https://github.com/eigeen/hd2-mod-crates/releases";

const MIN_LARGE_MIGRATION_TARGETS = 5;
const DESKTOP_RECOVERABLE_WEB_ERRORS = new Set([
  "NotAllowedError",
  "NotReadableError",
  "QuotaExceededError",
  "SecurityError",
]);

/** Recommend native processing once a task reaches half of its browser-specific limit. */
export function desktopRecommendationThreshold(webMappingLimit: number): number {
  const normalizedLimit = Math.max(1, Math.floor(webMappingLimit));
  const halfLimit = Math.ceil(normalizedLimit / 2);
  return Math.min(normalizedLimit, Math.max(MIN_LARGE_MIGRATION_TARGETS, halfLimit));
}

export function shouldRecommendDesktop(
  selectedTargetCount: number,
  webMappingLimit: number,
): boolean {
  return selectedTargetCount >= desktopRecommendationThreshold(webMappingLimit);
}

/** Identify failures caused by browser access and storage limits. */
export function isDesktopRecoverableWebError(error: unknown): boolean {
  if (!error || typeof error !== "object") return false;
  const name = (error as { name?: unknown }).name;
  return typeof name === "string" && DESKTOP_RECOVERABLE_WEB_ERRORS.has(name);
}

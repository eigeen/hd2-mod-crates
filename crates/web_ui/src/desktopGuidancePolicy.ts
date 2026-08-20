export const DESKTOP_RELEASE_URL = "https://github.com/eigeen/hd2-mod-crates/releases";

const MIN_LARGE_MIGRATION_TARGETS = 5;

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

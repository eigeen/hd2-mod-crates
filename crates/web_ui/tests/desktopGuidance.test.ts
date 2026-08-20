import { describe, expect, test } from "bun:test";
import {
  DESKTOP_RELEASE_URL,
  desktopRecommendationThreshold,
  isDesktopRecoverableWebError,
  shouldRecommendDesktop,
} from "../src/desktopGuidancePolicy";

describe("desktop guidance policy", () => {
  test("uses half of the normal web mapping limit", () => {
    expect(desktopRecommendationThreshold(20)).toBe(10);
    expect(desktopRecommendationThreshold(13)).toBe(7);
  });

  test("still recommends desktop when a large Patch has a low limit", () => {
    expect(desktopRecommendationThreshold(4)).toBe(4);
    expect(desktopRecommendationThreshold(1)).toBe(1);
  });

  test("starts the recommendation at the calculated threshold", () => {
    expect(shouldRecommendDesktop(9, 20)).toBe(false);
    expect(shouldRecommendDesktop(10, 20)).toBe(true);
  });

  test("uses the repository release list", () => {
    expect(DESKTOP_RELEASE_URL).toBe(
      "https://github.com/eigeen/hd2-mod-crates/releases",
    );
  });

  test("recognizes errors caused by browser limits", () => {
    expect(isDesktopRecoverableWebError(namedError("NotAllowedError"))).toBe(true);
    expect(isDesktopRecoverableWebError(namedError("NotReadableError"))).toBe(true);
    expect(isDesktopRecoverableWebError(namedError("QuotaExceededError"))).toBe(true);
    expect(isDesktopRecoverableWebError(namedError("SecurityError"))).toBe(true);
  });

  test("does not recommend desktop for cancellation or ordinary task errors", () => {
    expect(isDesktopRecoverableWebError(namedError("AbortError"))).toBe(false);
    expect(isDesktopRecoverableWebError(new Error("invalid Patch"))).toBe(false);
  });
});

function namedError(name: string): Error {
  const error = new Error(name);
  error.name = name;
  return error;
}

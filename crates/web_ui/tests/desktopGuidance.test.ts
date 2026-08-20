import { describe, expect, test } from "bun:test";
import {
  DESKTOP_RELEASE_URL,
  desktopRecommendationThreshold,
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
});

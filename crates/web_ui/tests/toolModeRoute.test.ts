import { describe, expect, test } from "bun:test";
import { hashForToolMode, toolModeFromHash } from "../src/toolModeRoute";

describe("tool mode hash routing", () => {
  test("resolves canonical routes", () => {
    expect(toolModeFromHash("#/migrate")).toBe("migrate");
    expect(toolModeFromHash("#/repatch")).toBe("repatch");
    expect(toolModeFromHash("#/merge")).toBe("merge");
  });

  test("accepts legacy hashes and trailing slashes", () => {
    expect(toolModeFromHash("#repatch")).toBe("repatch");
    expect(toolModeFromHash("#/merge/")).toBe("merge");
  });

  test("uses migrate for empty or unknown routes", () => {
    expect(toolModeFromHash("")).toBe("migrate");
    expect(toolModeFromHash("#/unknown")).toBe("migrate");
  });

  test("generates canonical hashes", () => {
    expect(hashForToolMode("migrate")).toBe("#/migrate");
    expect(hashForToolMode("repatch")).toBe("#/repatch");
    expect(hashForToolMode("merge")).toBe("#/merge");
  });
});

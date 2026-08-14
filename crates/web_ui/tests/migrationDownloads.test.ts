import { expect, test } from "bun:test";
import { uniqueOutputFilename } from "../src/migrationDownloads";
import type { TargetOption } from "../src/types";

const targets: TargetOption[] = [{
  category: "Armor",
  hash: "target-hash",
  name: "Target/Name",
  excluded: false,
}];

test("combines and sanitizes source and target names", () => {
  expect(uniqueOutputFilename("Original:Mod.zip", "target-hash", targets))
    .toBe("Original_Mod_Target_Name.zip");
});

test("uses the target hash when target metadata is unavailable", () => {
  expect(uniqueOutputFilename("Original", "missing-hash", targets))
    .toBe("Original_missing-hash.zip");
});

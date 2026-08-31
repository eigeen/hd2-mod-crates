import { describe, expect, test } from "bun:test";
import {
  moveDesktopMergeRecord,
  type DesktopMergeRecord,
} from "../src/desktopMergeRecords";

describe("desktop merge records", () => {
  test("moves a patch by one position without mutating the input", () => {
    const records = [record("first"), record("second"), record("third")];
    const moved = moveDesktopMergeRecord(records, "second", -1);

    expect(moved.map((item) => item.id)).toEqual(["second", "first", "third"]);
    expect(records.map((item) => item.id)).toEqual(["first", "second", "third"]);
  });

  test("keeps the same array when the move is outside the list", () => {
    const records = [record("first"), record("second")];

    expect(moveDesktopMergeRecord(records, "first", -1)).toBe(records);
    expect(moveDesktopMergeRecord(records, "second", 1)).toBe(records);
  });
});

function record(id: string): DesktopMergeRecord {
  return { byteLength: 1, id, name: `${id}.patch`, paths: [id] };
}

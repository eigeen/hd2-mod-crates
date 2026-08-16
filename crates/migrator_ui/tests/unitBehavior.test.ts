import { describe, expect, test } from "bun:test";
import {
  emptyUnitBehavior,
  mappingIsEnabled,
  preferredConflictSource,
  resetUnitBehavior,
  resolvedUnitExport,
  setMappingsEnabled,
  setPreferredConflictSource,
  setUnitsExported,
} from "../src/unitBehavior";

const mapping = { sourceFileId: "0000000000000001", targetFileId: "0000000000000009" };

describe("Unit behavior overrides", () => {
  test("disables and restores one conversion edge", () => {
    const disabled = setMappingsEnabled(emptyUnitBehavior(), [mapping], false);
    expect(mappingIsEnabled(disabled, mapping)).toBeFalse();

    const enabled = setMappingsEnabled(disabled, [mapping], true);
    expect(mappingIsEnabled(enabled, mapping)).toBeTrue();
  });

  test("overrides output independently from its global default", () => {
    const output = [{ fileId: mapping.targetFileId, defaultExport: true }];
    const excluded = setUnitsExported(emptyUnitBehavior(), output, false);
    expect(resolvedUnitExport(excluded, mapping.targetFileId, true)).toBeFalse();
    expect(resolvedUnitExport(excluded, "another", true)).toBeTrue();

    const restored = setUnitsExported(excluded, output, true);
    expect(restored.exportOverrides).toEqual([]);
  });

  test("selects and clears a preferred conflict source", () => {
    const preferred = setPreferredConflictSource(
      emptyUnitBehavior(),
      mapping.targetFileId,
      mapping.sourceFileId,
    );
    expect(preferredConflictSource(preferred, mapping.targetFileId)).toBe(mapping.sourceFileId);

    const cleared = setPreferredConflictSource(preferred, mapping.targetFileId, null);
    expect(preferredConflictSource(cleared, mapping.targetFileId)).toBeNull();
  });

  test("resets only behavior associated with the selected Unit", () => {
    const otherMapping = { sourceFileId: "2", targetFileId: "8" };
    const behavior = setPreferredConflictSource(
      setUnitsExported(
        setMappingsEnabled(emptyUnitBehavior(), [mapping, otherMapping], false),
        [
          { fileId: mapping.targetFileId, defaultExport: true },
          { fileId: otherMapping.targetFileId, defaultExport: true },
        ],
        false,
      ),
      mapping.targetFileId,
      mapping.sourceFileId,
    );
    const reset = resetUnitBehavior(
      behavior,
      [mapping],
      [mapping.targetFileId],
      mapping.targetFileId,
    );

    expect(mappingIsEnabled(reset, mapping)).toBeTrue();
    expect(mappingIsEnabled(reset, otherMapping)).toBeFalse();
    expect(reset.exportOverrides).toEqual([{ fileId: otherMapping.targetFileId, export: false }]);
    expect(reset.conflictResolutions).toEqual([]);
  });
});

import type {
  UnitBehaviorOptions,
  UnitMappingBehaviorKey,
} from "./types";

export function emptyUnitBehavior(): UnitBehaviorOptions {
  return { disabledMappings: [], exportOverrides: [], conflictResolutions: [] };
}

export function mappingIsEnabled(
  behavior: UnitBehaviorOptions,
  mapping: UnitMappingBehaviorKey,
): boolean {
  return !behavior.disabledMappings.some((candidate) => sameMapping(candidate, mapping));
}

export function setMappingsEnabled(
  behavior: UnitBehaviorOptions,
  mappings: UnitMappingBehaviorKey[],
  enabled: boolean,
): UnitBehaviorOptions {
  const changed = new Set(mappings.map(mappingKey));
  const retained = behavior.disabledMappings.filter((mapping) => !changed.has(mappingKey(mapping)));
  return {
    ...behavior,
    disabledMappings: enabled ? retained : [...retained, ...mappings],
  };
}

export function resolvedUnitExport(
  behavior: UnitBehaviorOptions,
  fileId: string,
  defaultValue: boolean,
): boolean {
  return behavior.exportOverrides.find((override) => override.fileId === fileId)?.export ?? defaultValue;
}

export function setUnitsExported(
  behavior: UnitBehaviorOptions,
  outputs: Array<{ fileId: string; defaultExport: boolean }>,
  exported: boolean,
): UnitBehaviorOptions {
  const changed = new Set(outputs.map((output) => output.fileId));
  const overrides = outputs
    .filter((output) => output.defaultExport !== exported)
    .map((output) => ({ fileId: output.fileId, export: exported }));
  return {
    ...behavior,
    exportOverrides: [
      ...behavior.exportOverrides.filter((override) => !changed.has(override.fileId)),
      ...overrides,
    ],
  };
}

export function preferredConflictSource(
  behavior: UnitBehaviorOptions,
  targetFileId: string,
): string | null {
  return behavior.conflictResolutions.find(
    (resolution) => resolution.targetFileId === targetFileId,
  )?.preferredSourceFileId ?? null;
}

export function setPreferredConflictSource(
  behavior: UnitBehaviorOptions,
  targetFileId: string,
  sourceFileId: string | null,
): UnitBehaviorOptions {
  const retained = behavior.conflictResolutions.filter(
    (resolution) => resolution.targetFileId !== targetFileId,
  );
  return {
    ...behavior,
    conflictResolutions: sourceFileId
      ? [...retained, { targetFileId, preferredSourceFileId: sourceFileId }]
      : retained,
  };
}

export function resetUnitBehavior(
  behavior: UnitBehaviorOptions,
  mappings: UnitMappingBehaviorKey[],
  fileIds: string[],
  conflictTargetFileId: string | null,
): UnitBehaviorOptions {
  const mappingKeys = new Set(mappings.map(mappingKey));
  const unitIds = new Set(fileIds);
  return {
    disabledMappings: behavior.disabledMappings.filter(
      (mapping) => !mappingKeys.has(mappingKey(mapping)),
    ),
    exportOverrides: behavior.exportOverrides.filter(
      (override) => !unitIds.has(override.fileId),
    ),
    conflictResolutions: behavior.conflictResolutions.filter(
      (resolution) => resolution.targetFileId !== conflictTargetFileId,
    ),
  };
}

export function hasUnitBehavior(
  behavior: UnitBehaviorOptions,
  mappings: UnitMappingBehaviorKey[],
  fileIds: string[],
  conflictTargetFileId: string | null,
): boolean {
  const mappingKeys = new Set(mappings.map(mappingKey));
  const unitIds = new Set(fileIds);
  return behavior.disabledMappings.some((mapping) => mappingKeys.has(mappingKey(mapping)))
    || behavior.exportOverrides.some((override) => unitIds.has(override.fileId))
    || behavior.conflictResolutions.some(
      (resolution) => resolution.targetFileId === conflictTargetFileId,
    );
}

function sameMapping(left: UnitMappingBehaviorKey, right: UnitMappingBehaviorKey): boolean {
  return left.sourceFileId === right.sourceFileId && left.targetFileId === right.targetFileId;
}

function mappingKey(mapping: UnitMappingBehaviorKey): string {
  return `${mapping.sourceFileId}>${mapping.targetFileId}`;
}

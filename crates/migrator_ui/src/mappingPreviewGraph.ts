import type {
  EquipmentMappingPreview,
  GraphEquipment,
  UnitBehaviorOptions,
  UnitMappingBehaviorKey,
} from "./types";
import {
  mappingIsEnabled,
  preferredConflictSource,
  resolvedUnitExport,
} from "./unitBehavior";

export interface MappingClaim {
  id: string;
  mappingId: string;
  sourceEquipment: GraphEquipment;
  targetEquipment: GraphEquipment;
  sourceUnitId: string;
  sourceFileId: string;
  targetUnitId: string;
  targetFileId: string;
}

export interface MappingConflict {
  preferredSourceFileId: string | null;
  sourceFileIds: string[];
  sourceUnitIds: string[];
  state: "unresolved" | "resolved";
  targetFileId: string;
  targetUnitId: string;
}

export interface MappingGraphAnalysis {
  claims: MappingClaim[];
  conflictsByTarget: ReadonlyMap<string, MappingConflict>;
  sharedUnitIds: ReadonlySet<string>;
}

export interface MappingGraphSummary {
  conflictCount: number;
  mappingCount: number;
  sharedUnitCount: number;
  unitCount: number;
}

/** Flattens batch previews into canonical claims and derives actual cross-mapping conflicts. */
export function analyzeMappingGraph(
  previews: EquipmentMappingPreview[],
  behavior: UnitBehaviorOptions,
): MappingGraphAnalysis {
  const claims = mappingClaims(previews);
  return {
    claims,
    conflictsByTarget: collectConflicts(claims, behavior),
    sharedUnitIds: mappingSharedUnitIds(claims),
  };
}

export function summarizeMappingGraph(
  previews: EquipmentMappingPreview[],
  behavior: UnitBehaviorOptions,
): MappingGraphSummary {
  const analysis = analyzeMappingGraph(previews, behavior);
  const unitIds = new Set(analysis.claims.flatMap((claim) => [claim.sourceUnitId, claim.targetUnitId]));
  return {
    conflictCount: analysis.conflictsByTarget.size,
    mappingCount: previews.length,
    sharedUnitCount: analysis.sharedUnitIds.size,
    unitCount: unitIds.size,
  };
}

export function mappingClaims(previews: EquipmentMappingPreview[]): MappingClaim[] {
  return previews.flatMap((preview, previewIndex) => {
    const fileIds = new Map(preview.units.map((unit) => [unit.id, unit.fileId]));
    return preview.mappings.map((mapping, mappingIndex) => ({
      id: `claim:${previewIndex}:${mappingIndex}:${mapping.id}`,
      mappingId: mapping.id,
      sourceEquipment: preview.sourceEquipment,
      targetEquipment: preview.targetEquipment,
      sourceUnitId: mapping.sourceUnitId,
      sourceFileId: requiredFileId(fileIds, mapping.sourceUnitId),
      targetUnitId: mapping.targetUnitId,
      targetFileId: requiredFileId(fileIds, mapping.targetUnitId),
    }));
  });
}

export function claimBehaviorKey(claim: MappingClaim): UnitMappingBehaviorKey {
  return { sourceFileId: claim.sourceFileId, targetFileId: claim.targetFileId };
}

function collectConflicts(
  claims: MappingClaim[],
  behavior: UnitBehaviorOptions,
): ReadonlyMap<string, MappingConflict> {
  const byTarget = groupClaimsByTarget(activeClaims(claims, behavior));
  const conflicts = new Map<string, MappingConflict>();
  byTarget.forEach((targetClaims, targetUnitId) => {
    if (!containsIncompatibleClaims(targetClaims)) return;
    const sourceFileIds = unique(targetClaims.map((claim) => claim.sourceFileId));
    const preferred = preferredConflictSource(behavior, targetClaims[0].targetFileId);
    conflicts.set(targetUnitId, {
      preferredSourceFileId: preferred,
      sourceFileIds,
      sourceUnitIds: unique(targetClaims.map((claim) => claim.sourceUnitId)),
      state: preferred && sourceFileIds.includes(preferred) ? "resolved" : "unresolved",
      targetFileId: targetClaims[0].targetFileId,
      targetUnitId,
    });
  });
  return conflicts;
}

function activeClaims(
  claims: MappingClaim[],
  behavior: UnitBehaviorOptions,
): MappingClaim[] {
  return claims.filter((claim) => (
    mappingIsEnabled(behavior, claimBehaviorKey(claim))
      && resolvedUnitExport(behavior, claim.targetFileId, true)
  ));
}

function groupClaimsByTarget(claims: MappingClaim[]): Map<string, MappingClaim[]> {
  const groups = new Map<string, MappingClaim[]>();
  claims.forEach((claim) => {
    const group = groups.get(claim.targetUnitId) ?? [];
    group.push(claim);
    groups.set(claim.targetUnitId, group);
  });
  return groups;
}

function containsIncompatibleClaims(claims: MappingClaim[]): boolean {
  for (let left = 0; left < claims.length; left += 1) {
    for (let right = left + 1; right < claims.length; right += 1) {
      if (!claimsAreCompatible(claims[left], claims[right])) return true;
    }
  }
  return false;
}

/** Returns structural conflict targets without applying mutable Unit behavior. */
export function mappingConflictTargetIds(claims: MappingClaim[]): ReadonlySet<string> {
  return new Set(
    [...groupClaimsByTarget(claims)]
      .filter(([, targetClaims]) => containsIncompatibleClaims(targetClaims))
      .map(([targetUnitId]) => targetUnitId),
  );
}

/** Mirrors backend target-owner validation so the graph reports only executable conflicts. */
function claimsAreCompatible(left: MappingClaim, right: MappingClaim): boolean {
  if (left.sourceFileId === right.sourceFileId) return true;
  return left.sourceEquipment.category === right.sourceEquipment.category
    && left.sourceEquipment.hash === right.sourceEquipment.hash
    && left.targetEquipment.hash !== right.targetEquipment.hash;
}

export function mappingSharedUnitIds(claims: MappingClaim[]): ReadonlySet<string> {
  const equipmentByUnit = new Map<string, Set<string>>();
  claims.forEach((claim) => {
    addEquipmentReference(equipmentByUnit, claim.sourceUnitId, claim.sourceEquipment.id);
    addEquipmentReference(equipmentByUnit, claim.targetUnitId, claim.targetEquipment.id);
  });
  return new Set(
    [...equipmentByUnit]
      .filter(([, equipmentIds]) => equipmentIds.size > 1)
      .map(([unitId]) => unitId),
  );
}

function addEquipmentReference(
  references: Map<string, Set<string>>,
  unitId: string,
  equipmentId: string,
) {
  const equipmentIds = references.get(unitId) ?? new Set<string>();
  equipmentIds.add(equipmentId);
  references.set(unitId, equipmentIds);
}

function requiredFileId(fileIds: ReadonlyMap<string, string>, unitId: string): string {
  const fileId = fileIds.get(unitId);
  if (!fileId) throw new Error(`Mapping preview is missing Unit ${unitId}`);
  return fileId;
}

function unique(values: string[]): string[] {
  return [...new Set(values)];
}

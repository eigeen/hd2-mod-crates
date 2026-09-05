import {
  migrationMappingLabel,
  uniqueOutputFilename,
  type EquipmentOption,
  type MigrationVariant,
  type UnifiedMigrateOptions,
  type UnitBehaviorOptions,
  type UnmatchedUnitPolicy,
} from "@hd2-mod-tools/migrator-ui";
import type { SelectedPatch } from "./platform";

export const MIGRATION_PATCH_SUFFIX = "9ba626afa44a3aa3.patch_0";
export const DEFAULT_MIGRATION_OUTPUT = "hd2-migrated-patch.zip";
export const DEFAULT_REPATCH_OUTPUT = "hd2-repatched-mod.zip";

interface CompileMigrationRequest<Source> {
  equipmentOptions: EquipmentOption[];
  noPadding: boolean;
  patch: SelectedPatch<Source>;
  unitBehavior: UnitBehaviorOptions;
  unmatchedUnitPolicy: UnmatchedUnitPolicy;
  variants: MigrationVariant[];
}

export interface CompiledMigrationRequest {
  options: UnifiedMigrateOptions;
  progressLabels: string[];
  suggestedOutputFilename: string;
}

/** Compile product rules once before handing work to a platform adapter. */
export function compileMigrationRequest<Source>(
  request: CompileMigrationRequest<Source>,
): CompiledMigrationRequest {
  const mappings = request.variants.flatMap((variant) => variant.mappings);
  return {
    options: {
      variants: request.variants,
      patchSuffix: MIGRATION_PATCH_SUFFIX,
      noPadding: request.noPadding,
      unmatchedUnitPolicy: request.unmatchedUnitPolicy,
      unitBehavior: request.unitBehavior,
    },
    progressLabels: mappings.map((mapping) => migrationMappingLabel(mapping, request.equipmentOptions)),
    suggestedOutputFilename: migrationOutputFilename(request.patch, request.variants, request.equipmentOptions),
  };
}

function migrationOutputFilename<Source>(
  patch: SelectedPatch<Source>,
  variants: MigrationVariant[],
  equipmentOptions: EquipmentOption[],
): string {
  const mapping = variants.length === 1 && variants[0].mappings.length === 1
    ? variants[0].mappings[0]
    : null;
  if (!mapping) return DEFAULT_MIGRATION_OUTPUT;
  return uniqueOutputFilename(
    patch.originalName ?? patch.info.name,
    mapping.targetHash,
    equipmentOptions,
  );
}

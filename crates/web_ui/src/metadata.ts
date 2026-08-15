import type { AuthorityMappings } from "@hd2-mod-tools/migrator-ui";

const AUTHORITY_MAPPINGS_URL = "/assets/armor_mappings.merged.json";

export async function loadAuthorityMappings(): Promise<AuthorityMappings> {
  const response = await fetch(AUTHORITY_MAPPINGS_URL);
  if (!response.ok) {
    throw new Error(`加载权威映射失败：${response.status} ${response.statusText}`);
  }
  const raw = (await response.json()) as Record<string, unknown>;
  const names = new Set(Object.keys(raw));
  return {
    armorCount: names.size,
    hasArmor: (name) => names.has(name),
  };
}

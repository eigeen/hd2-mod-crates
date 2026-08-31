export interface DesktopMergeRecord {
  byteLength: number;
  id: string;
  name: string;
  paths: string[];
}

export function moveDesktopMergeRecord(
  records: DesktopMergeRecord[],
  id: string,
  offset: number,
): DesktopMergeRecord[] {
  const from = records.findIndex((record) => record.id === id);
  const to = from + offset;
  if (from < 0 || to < 0 || to >= records.length) return records;
  const next = [...records];
  const [record] = next.splice(from, 1);
  next.splice(to, 0, record);
  return next;
}

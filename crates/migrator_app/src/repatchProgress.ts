import { throwIfTaskCancelled, type Translate } from "@hd2-mod-tools/migrator-ui";

/** Translate shared repair stages into progress labels for either host. */
export function createRepatchProgress(
  t: Translate,
  signal: AbortSignal,
  onProgressLabel: (label: string) => void,
  now: () => number = () => performance.now(),
): (stage: string) => void {
  let lastScanUpdate = -Infinity;
  return (stage) => {
    throwIfTaskCancelled(signal);
    if (stage === "reading_patch") onProgressLabel(t("repatch.readingPatch"));
    if (stage === "opening_game_data") onProgressLabel(t("repatch.openingGameData"));
    if (stage === "repatching_units") onProgressLabel(t("repatch.updatingMod"));
    if (stage === "saving_output") onProgressLabel(t("repatch.savingOutput"));
    const scan = /^scan_archives:(\d+):(\d+)$/.exec(stage);
    if (scan) {
      const timestamp = now();
      if (timestamp - lastScanUpdate < 100 && scan[1] !== scan[2]) return;
      lastScanUpdate = timestamp;
      onProgressLabel(t("repatch.scanningGameData", { current: scan[1], total: scan[2] }));
    }
  };
}

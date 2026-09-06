import { throwIfTaskCancelled, type Translate } from "@hd2-mod-tools/migrator-ui";

/** Translate shared repair stages into progress labels for either host. */
export function createRepatchProgress(
  t: Translate,
  signal: AbortSignal,
  onProgressLabel: (label: string) => void,
): (stage: string) => void {
  return (stage) => {
    throwIfTaskCancelled(signal);
    if (stage === "reading_patch") onProgressLabel(t("repatch.readingPatch"));
    if (stage === "opening_game_data") onProgressLabel(t("repatch.openingGameData"));
    if (stage === "repatching_units") onProgressLabel(t("repatch.updatingMod"));
    if (stage === "saving_output") onProgressLabel(t("repatch.savingOutput"));
    const scan = /^scan_archives:(\d+):(\d+)$/.exec(stage);
    if (scan) {
      onProgressLabel(t("repatch.scanningGameData", { current: scan[1], total: scan[2] }));
    }
  };
}

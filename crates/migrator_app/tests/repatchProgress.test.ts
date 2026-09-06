import { describe, expect, test } from "bun:test";
import type { Translate } from "@hd2-mod-tools/migrator-ui";
import { createRepatchProgress } from "../src/repatchProgress";

const translate: Translate = (key, values) => `${key}:${JSON.stringify(values ?? {})}`;

describe("repair progress", () => {
  test("reports scan counts without exposing unknown core messages", () => {
    const labels: string[] = [];
    const stage = createRepatchProgress(translate, new AbortController().signal, (label) => labels.push(label));
    stage("opening_game_data");
    stage("scan_archives:125:3600");
    stage("scanning 1234567890abcdef");
    stage("repatching_units");
    expect(labels).toEqual([
      "repatch.openingGameData:{}",
      'repatch.scanningGameData:{"current":"125","total":"3600"}',
      "repatch.updatingMod:{}",
    ]);
  });

  test("a cancelled scan cannot update progress", () => {
    const controller = new AbortController();
    controller.abort();
    const labels: string[] = [];
    const stage = createRepatchProgress(translate, controller.signal, (label) => labels.push(label));
    expect(() => stage("scan_archives:2:3")).toThrow();
    expect(labels).toEqual([]);
  });
});

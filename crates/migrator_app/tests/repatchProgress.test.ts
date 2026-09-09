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

  test("limits scan rendering while keeping stage changes and cancellation immediate", () => {
    const controller = new AbortController();
    const labels: string[] = [];
    let time = 0;
    const stage = createRepatchProgress(translate, controller.signal, (label) => labels.push(label), () => time);
    stage("scan_archives:1:1000");
    for (let count = 2; count < 100; count += 1) stage(`scan_archives:${count}:1000`);
    expect(labels).toHaveLength(1);
    time = 100;
    stage("scan_archives:100:1000");
    stage("repatching_units");
    expect(labels).toHaveLength(3);
    expect(labels[2]).toBe("repatch.updatingMod:{}");
    controller.abort();
    expect(() => stage("scan_archives:101:1000")).toThrow();
  });
});

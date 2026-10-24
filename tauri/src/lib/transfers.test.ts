import { describe, expect, it } from "vitest";
import { downloadIsActive, downloadPercent, liveUploads } from "$lib/transfers";
import type { DownloadProgress, UploadSnapshot } from "$lib/types";

function dl(over: Partial<DownloadProgress>): DownloadProgress {
  return {
    install_token: "t",
    source_device_id: "d",
    source_device_name: "Deck",
    source_game_id: "g",
    game_name: "Game",
    bytes_done: 0,
    bytes_total: 0,
    current_file: "",
    status: "starting",
    message: null,
    new_game_id: null,
    bytes_per_second: 0,
    ...over,
  } as DownloadProgress;
}

function up(id: string, cancelled: boolean): UploadSnapshot {
  return { session_id: id, cancelled } as UploadSnapshot;
}

describe("downloadIsActive", () => {
  it("is false with no download", () => {
    expect(downloadIsActive(null)).toBe(false);
  });

  it("is true while starting or transferring", () => {
    expect(downloadIsActive(dl({ status: "starting" }))).toBe(true);
    expect(downloadIsActive(dl({ status: "transferring" }))).toBe(true);
  });

  it("is false once finished, failed or canceled", () => {
    for (const status of ["done", "error", "canceled"] as const) {
      expect(downloadIsActive(dl({ status }))).toBe(false);
    }
  });
});

describe("downloadPercent", () => {
  it("is 0 with no download or an unknown total", () => {
    expect(downloadPercent(null)).toBe(0);
    expect(downloadPercent(dl({ bytes_done: 50, bytes_total: 0 }))).toBe(0);
  });

  it("rounds to whole percent", () => {
    expect(downloadPercent(dl({ bytes_done: 1, bytes_total: 3 }))).toBe(33);
    expect(downloadPercent(dl({ bytes_done: 2, bytes_total: 3 }))).toBe(67);
    expect(downloadPercent(dl({ bytes_done: 100, bytes_total: 100 }))).toBe(100);
  });
});

describe("liveUploads", () => {
  it("drops cancelled sessions and keeps order", () => {
    const out = liveUploads([up("a", false), up("b", true), up("c", false)]);
    expect(out.map((u) => u.session_id)).toEqual(["a", "c"]);
  });

  it("handles an empty list", () => {
    expect(liveUploads([])).toEqual([]);
  });
});

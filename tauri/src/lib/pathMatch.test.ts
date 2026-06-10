import { describe, expect, it } from "vitest";
import {
  normPath,
  parentOf,
  canonPath,
  isCurrentRoot,
  neededBytes,
} from "$lib/pathMatch";

describe("normPath", () => {
  it("strips trailing separators", () => {
    expect(normPath("/games/")).toBe("/games");
    expect(normPath("C:\\Games\\")).toBe("C:\\Games");
    expect(normPath("/games")).toBe("/games");
  });
});

describe("parentOf", () => {
  it("drops the last segment (posix)", () => {
    expect(parentOf("/mnt/games/Hades")).toBe("/mnt/games");
    expect(parentOf("/mnt/games/Hades/")).toBe("/mnt/games");
  });
  it("drops the last segment (windows)", () => {
    expect(parentOf("D:\\Games\\Hades")).toBe("D:\\Games");
  });
});

describe("canonPath", () => {
  it("lowercases and slash-folds windows paths", () => {
    expect(canonPath("D:\\Games\\Hades")).toBe("d:/games/hades");
    expect(canonPath("d:/GAMES/Hades")).toBe("d:/games/hades");
  });
  it("leaves posix paths case-sensitive", () => {
    expect(canonPath("/mnt/Games")).toBe("/mnt/Games");
    expect(canonPath("/mnt/games/")).toBe("/mnt/games");
  });
});

describe("isCurrentRoot", () => {
  it("matches the parent folder of the install", () => {
    expect(isCurrentRoot("/mnt/games", "/mnt/games/Hades")).toBe(true);
    expect(isCurrentRoot("/mnt/games/", "/mnt/games/Hades")).toBe(true);
  });
  it("does not match a different folder", () => {
    expect(isCurrentRoot("/mnt/other", "/mnt/games/Hades")).toBe(false);
    // Sibling, not parent.
    expect(isCurrentRoot("/mnt/games/Hades", "/mnt/games/Hades")).toBe(false);
  });
  it("folds windows casing and separators", () => {
    expect(isCurrentRoot("D:\\Games", "d:/games/Hades")).toBe(true);
    expect(isCurrentRoot("d:/games", "D:\\Games\\Hades")).toBe(true);
  });
  it("returns false when there is no install folder", () => {
    expect(isCurrentRoot("/mnt/games", null)).toBe(false);
    expect(isCurrentRoot("/mnt/games", undefined)).toBe(false);
    expect(isCurrentRoot("/mnt/games", "")).toBe(false);
  });
});

describe("neededBytes", () => {
  it("reserves a 256 MiB floor for small installs", () => {
    expect(neededBytes(0)).toBe(256 * 1048576);
    expect(neededBytes(100 * 1048576)).toBe(100 * 1048576 + 256 * 1048576);
  });
  it("reserves 1% for large installs", () => {
    const big = 100 * 1024 * 1048576; // 100 GiB
    expect(neededBytes(big)).toBe(big + big / 100);
  });
});

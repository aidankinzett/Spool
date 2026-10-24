import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import {
  absDate,
  absDateTime,
  isNewerVersion,
  fmtCatalog,
  fmtPlaytime,
  fmtRate,
  fmtSize,
  relDate,
  parentDir,
  folderForGame,
} from "$lib/format";

describe("relDate", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-05-28T12:00:00Z"));
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns an em dash for null/undefined/invalid", () => {
    expect(relDate(null)).toBe("—");
    expect(relDate(undefined)).toBe("—");
    expect(relDate("not-a-date")).toBe("—");
  });

  it("renders sub-minute as 'just now'", () => {
    expect(relDate("2026-05-28T11:59:30Z")).toBe("just now");
  });

  it("renders minutes, hours and days", () => {
    expect(relDate("2026-05-28T11:30:00Z")).toBe("30m ago");
    expect(relDate("2026-05-28T09:00:00Z")).toBe("3h ago");
    expect(relDate("2026-05-25T12:00:00Z")).toBe("3d ago");
  });

  it("renders weeks and months past the day boundaries", () => {
    expect(relDate("2026-05-14T12:00:00Z")).toBe("2w ago");
    expect(relDate("2026-03-28T12:00:00Z")).toBe("2mo ago");
  });
});

describe("fmtPlaytime", () => {
  it("returns an em dash for zero/null", () => {
    expect(fmtPlaytime(0)).toBe("—");
    expect(fmtPlaytime(null)).toBe("—");
  });

  it("renders minutes-only and hours+minutes", () => {
    expect(fmtPlaytime(12)).toBe("12m");
    expect(fmtPlaytime(323)).toBe("5h 23m");
  });

  it("drops minutes once over 100 hours", () => {
    expect(fmtPlaytime(6_001)).toBe("100h");
  });
});

describe("fmtSize", () => {
  it("returns an em dash for zero/null", () => {
    expect(fmtSize(0)).toBe("—");
    expect(fmtSize(null)).toBe("—");
  });

  it("renders MB below 1024 and GB at/above", () => {
    expect(fmtSize(423)).toBe("423.0 MB");
    expect(fmtSize(2048)).toBe("2.0 GB");
  });

  it("drops sub-MB sizes to KB", () => {
    expect(fmtSize(0.015625)).toBe("16 KB"); // 16 KB backup
    expect(fmtSize(0.5)).toBe("512 KB");
    expect(fmtSize(0.0001)).toBe("0.1 KB"); // ~102 bytes
  });

  it("rounds up across the KB/MB/GB boundaries cleanly", () => {
    expect(fmtSize(0.9999)).toBe("1.0 MB");
    expect(fmtSize(1023.99)).toBe("1.0 GB");
  });
});

describe("fmtRate", () => {
  it("returns an ellipsis for zero/null", () => {
    expect(fmtRate(0)).toBe("…");
    expect(fmtRate(null)).toBe("…");
  });

  it("reports bitrate (×8) scaling across bps, Kbps, Mbps, Gbps", () => {
    expect(fmtRate(100)).toBe("800 bps");
    expect(fmtRate(1000)).toBe("8.0 Kbps");
    expect(fmtRate(1_000_000)).toBe("8.0 Mbps");
    expect(fmtRate(1_000_000_000)).toBe("8.00 Gbps");
  });
});

describe("fmtCatalog", () => {
  it("zero-pads to four digits with the SPL prefix", () => {
    expect(fmtCatalog(42)).toBe("SPL-0042");
    expect(fmtCatalog(12345)).toBe("SPL-12345");
  });
});

describe("absDate", () => {
  it("returns an em dash for null/invalid", () => {
    expect(absDate(null)).toBe("—");
    expect(absDate("nope")).toBe("—");
  });

  it("renders a UK-style absolute date", () => {
    expect(absDate("2026-05-26T00:00:00Z")).toBe("26 May 2026");
  });
});

describe("parentDir", () => {
  it("returns null for null/undefined/empty", () => {
    expect(parentDir(null)).toBeNull();
    expect(parentDir(undefined)).toBeNull();
    expect(parentDir("")).toBeNull();
  });

  it("returns null when no separator is found", () => {
    expect(parentDir("game.exe")).toBeNull();
  });

  it("handles Unix path separation", () => {
    expect(parentDir("/usr/bin/spool")).toBe("/usr/bin");
    expect(parentDir("/game.exe")).toBe("/");
    expect(parentDir("foo/bar")).toBe("foo");
  });

  it("handles Windows path separation", () => {
    expect(parentDir("C:\\Program Files\\Spool\\game.exe")).toBe("C:\\Program Files\\Spool");
    expect(parentDir("C:\\game.exe")).toBe("C:\\");
    expect(parentDir("C:/game.exe")).toBe("C:/");
    expect(parentDir("\\\\server\\share\\game.exe")).toBe("\\\\server\\share");
    expect(parentDir(".\\game.exe")).toBe(".");
  });
});

describe("folderForGame", () => {
  it("returns game_folder_path if defined", () => {
    expect(folderForGame({ game_folder_path: "/override/path", exe_path: "/usr/bin/spool" })).toBe("/override/path");
  });

  it("returns parent of exe_path if game_folder_path is missing", () => {
    expect(folderForGame({ game_folder_path: null, exe_path: "/usr/bin/spool" })).toBe("/usr/bin");
    expect(folderForGame({ game_folder_path: undefined, exe_path: "C:\\game.exe" })).toBe("C:\\");
  });

  it("returns null if neither is available", () => {
    expect(folderForGame({ game_folder_path: null, exe_path: null })).toBeNull();
    expect(folderForGame({})).toBeNull();
  });
});

describe("isNewerVersion", () => {
  it("compares numerically segment by segment, not as strings", () => {
    expect(isNewerVersion("1.10.0", "1.4.0")).toBe(true);
    expect(isNewerVersion("1.4.0", "1.10.0")).toBe(false);
    expect(isNewerVersion("2.0.0", "1.99.99")).toBe(true);
  });

  it("is false for equal versions and treats missing segments as zero", () => {
    expect(isNewerVersion("1.2.3", "1.2.3")).toBe(false);
    expect(isNewerVersion("1.2", "1.2.0")).toBe(false);
    expect(isNewerVersion("1.2.0.1", "1.2")).toBe(true);
    expect(isNewerVersion("1.2", "1.2.0.1")).toBe(false);
  });

  it("counts non-numeric segments as zero instead of failing", () => {
    expect(isNewerVersion("1.x.1", "1.0.0")).toBe(true);
    expect(isNewerVersion("abc", "0")).toBe(false);
    expect(isNewerVersion("", "0.0.1")).toBe(false);
  });
});

describe("relDate boundaries", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-05-28T12:00:00Z"));
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("switches unit exactly at 60s, 1h, 1d, 7d and 30d", () => {
    expect(relDate("2026-05-28T11:59:01Z")).toBe("just now"); // 59s
    expect(relDate("2026-05-28T11:59:00Z")).toBe("1m ago"); // 60s
    expect(relDate("2026-05-28T11:00:01Z")).toBe("60m ago"); // 3599s rounds up
    expect(relDate("2026-05-28T11:00:00Z")).toBe("1h ago"); // 3600s
    expect(relDate("2026-05-27T12:00:00Z")).toBe("1d ago");
    expect(relDate("2026-05-21T12:00:00Z")).toBe("1w ago"); // 7 days
    expect(relDate("2026-04-28T12:00:00Z")).toBe("1mo ago"); // 30 days
  });

  it("treats a timestamp in the future as just now", () => {
    expect(relDate("2026-05-28T12:30:00Z")).toBe("just now");
  });
});

describe("absDateTime", () => {
  it("returns an em dash for empty or invalid input", () => {
    expect(absDateTime(null)).toBe("—");
    expect(absDateTime("")).toBe("—");
    expect(absDateTime("nope")).toBe("—");
  });

  it("renders a UK-style date with a 24-hour time", () => {
    // Local time zone dependent, so only the stable parts are checked.
    const out = absDateTime("2026-05-26T14:32:00");
    expect(out).toContain("26 May 2026");
    expect(out).toMatch(/14:32/);
  });
});

describe("fmtPlaytime boundaries", () => {
  it("shows hours and minutes up to 99h59m, then hours only", () => {
    expect(fmtPlaytime(60)).toBe("1h 0m");
    expect(fmtPlaytime(5_999)).toBe("99h 59m");
    expect(fmtPlaytime(6_000)).toBe("100h");
    expect(fmtPlaytime(60_000)).toBe("1000h");
  });
});

describe("fmtSize boundaries", () => {
  it("handles exactly 1 MB, 1 GB and values just under them", () => {
    expect(fmtSize(1)).toBe("1.0 MB");
    expect(fmtSize(1024)).toBe("1.0 GB");
    expect(fmtSize(1023.94)).toBe("1023.9 MB");
    expect(fmtSize(1023.96)).toBe("1.0 GB");
  });

  it("treats negative sizes like a real value rather than crashing", () => {
    expect(() => fmtSize(-5)).not.toThrow();
  });
});

describe("fmtRate boundaries", () => {
  it("moves up a unit at 1000 bits per second and ignores negatives", () => {
    expect(fmtRate(124)).toBe("992 bps");
    expect(fmtRate(125)).toBe("1.0 Kbps");
    expect(fmtRate(-1)).toBe("…");
  });
});

describe("parentDir edge cases", () => {
  it("handles trailing separators and mixed separators", () => {
    expect(parentDir("/a/b/")).toBe("/a/b");
    expect(parentDir("C:\\a/b\\c.exe")).toBe("C:\\a/b");
  });
});

import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import {
  absDate,
  fmtCatalog,
  fmtPlaytime,
  fmtRate,
  fmtSize,
  relDate,
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

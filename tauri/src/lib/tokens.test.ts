import { describe, expect, it } from "vitest";
import { shadeHex } from "$lib/tokens";

describe("shadeHex", () => {
  it("lightens and darkens by a percentage of 255", () => {
    expect(shadeHex("#000000", 10)).toBe("#1a1a1a");
    expect(shadeHex("#808080", -20)).toBe("#4d4d4d");
  });

  it("clamps each channel at 0 and 255", () => {
    expect(shadeHex("#ffffff", 50)).toBe("#ffffff");
    expect(shadeHex("#000000", -50)).toBe("#000000");
    expect(shadeHex("#ff0010", 10)).toBe("#ff1a2a");
  });

  it("expands 3-digit shorthand and accepts a missing #", () => {
    expect(shadeHex("#fff", -100)).toBe("#000000");
    expect(shadeHex("000", 100)).toBe("#ffffff");
  });

  it("zero shift returns the same colour, zero-padded", () => {
    expect(shadeHex("#0000ff", 0)).toBe("#0000ff");
    expect(shadeHex("#010203", 0)).toBe("#010203");
  });
});

import { describe, expect, it } from "vitest";
import { killstreakName, paintToHex, qualityColor, qualityName } from "./quality";

describe("qualityName", () => {
  it("returns the real Valve quality name for known ids", () => {
    expect(qualityName(6)).toBe("Unique");
    expect(qualityName(5)).toBe("Unusual");
    expect(qualityName(11)).toBe("Strange");
  });

  it("falls back to a generic label for unknown ids", () => {
    expect(qualityName(99)).toBe("Quality 99");
  });
});

describe("qualityColor", () => {
  it("returns a distinct CSS var per known quality", () => {
    expect(qualityColor(6)).toBe("var(--color-quality-unique)");
    expect(qualityColor(5)).toBe("var(--color-quality-unusual)");
  });

  it("falls back to normal for unknown ids", () => {
    expect(qualityColor(99)).toBe(qualityColor(0));
  });
});

describe("killstreakName", () => {
  it("names each tier", () => {
    expect(killstreakName(1)).toBe("Killstreak");
    expect(killstreakName(2)).toBe("Specialized Killstreak");
    expect(killstreakName(3)).toBe("Professional Killstreak");
  });

  it("is empty for tier 0 (no killstreak)", () => {
    expect(killstreakName(0)).toBe("");
  });
});

describe("paintToHex", () => {
  it("converts a raw RGB int to a lowercase hex color", () => {
    expect(paintToHex(0x7e7e7e)).toBe("#7e7e7e");
    expect(paintToHex(0xff0000)).toBe("#ff0000");
  });

  it("pads short values to 6 hex digits", () => {
    expect(paintToHex(0x1)).toBe("#000001");
  });
});

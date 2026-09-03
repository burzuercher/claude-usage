import { describe, it, expect } from "vitest";
import {
  fmtNum,
  fmtTokens,
  fmtDuration,
  fmtForecast,
  fmtResetRelative,
  fmtResetAbsolute,
  fmtAgo,
  clamp01,
} from "./format";

describe("fmtNum", () => {
  it("groups thousands", () => {
    expect(fmtNum(1842300)).toBe("1,842,300");
    expect(fmtNum(0)).toBe("0");
  });
});

describe("fmtTokens", () => {
  it("uses M for >= 1e6", () => {
    expect(fmtTokens(1_842_300)).toBe("1.84M");
    expect(fmtTokens(20_000_000)).toBe("20.00M");
  });
  it("uses k below 1e6", () => {
    expect(fmtTokens(42_800)).toBe("42.8k");
    expect(fmtTokens(1_000)).toBe("1.0k");
  });
});

describe("fmtDuration", () => {
  it("formats minutes only under an hour", () => {
    expect(fmtDuration(54 * 60_000)).toBe("54m");
    expect(fmtDuration(60_000)).toBe("1m");
  });
  it("formats hours and zero-padded minutes", () => {
    expect(fmtDuration((3 * 3600 + 12 * 60) * 1000)).toBe("3h 12m");
    expect(fmtDuration((2 * 3600 + 5 * 60) * 1000)).toBe("2h 05m");
  });
  it("clamps negatives to 0m", () => {
    expect(fmtDuration(-5000)).toBe("0m");
  });
});

describe("fmtForecast", () => {
  it("renders readable hours/minutes", () => {
    expect(fmtForecast(114)).toBe("1h 54m");
    expect(fmtForecast(54)).toBe("54m");
  });
  it("handles edges", () => {
    expect(fmtForecast(0.5)).toBe("under a minute");
    expect(fmtForecast(480)).toBe("8h+");
    expect(fmtForecast(999)).toBe("8h+");
    expect(fmtForecast(Infinity)).toBe("8h+");
  });
});

describe("clamp01", () => {
  it("clamps to [0,1]", () => {
    expect(clamp01(-1)).toBe(0);
    expect(clamp01(0.42)).toBe(0.42);
    expect(clamp01(1.7)).toBe(1);
  });
});

describe("reset formatters", () => {
  it("relative reset uses duration", () => {
    const now = Date.now();
    const iso = new Date(now + (3 * 3600 + 30 * 60) * 1000).toISOString();
    expect(fmtResetRelative(iso, now)).toBe("3h 30m");
  });
  it("empty / invalid inputs return empty string", () => {
    expect(fmtResetRelative("", Date.now())).toBe("");
    expect(fmtResetAbsolute("")).toBe("");
    expect(fmtResetAbsolute("not-a-date")).toBe("");
  });
  it("absolute reset includes a weekday and time", () => {
    const out = fmtResetAbsolute("2026-05-29T18:00:00.000Z");
    expect(out).toMatch(/^[A-Z][a-z]{2}\s/); // e.g. "Fri 1:00 PM"
    expect(out).toMatch(/(AM|PM)$/);
  });
});

describe("fmtAgo", () => {
  const now = 1_700_000_000_000;
  it("says just now under a minute", () => {
    expect(fmtAgo(now - 5_000, now)).toBe("just now");
    expect(fmtAgo(now + 5_000, now)).toBe("just now"); // clock skew → not negative
  });
  it("uses duration formatting under a day", () => {
    expect(fmtAgo(now - 3 * 60_000, now)).toBe("3m ago");
    expect(fmtAgo(now - (60 + 4) * 60_000, now)).toBe("1h 04m ago");
  });
  it("uses days beyond 24h", () => {
    expect(fmtAgo(now - 2.5 * 86_400_000, now)).toBe("2d ago");
  });
  it("returns empty for a missing timestamp", () => {
    expect(fmtAgo(0, now)).toBe("");
  });
});

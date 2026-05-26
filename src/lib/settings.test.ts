import { describe, it, expect, beforeEach } from "vitest";
import { DEFAULT_SETTINGS, loadSettings, saveSettings } from "./settings";

const KEY = "claude-usage.settings";

beforeEach(() => localStorage.clear());

describe("DEFAULT_SETTINGS", () => {
  it("has the expected privacy-friendly defaults", () => {
    expect(DEFAULT_SETTINGS.plan).toBe("max5");
    expect(DEFAULT_SETTINGS.autoPlan).toBe(true);
    expect(DEFAULT_SETTINGS.seats).toBe(5);
    expect(DEFAULT_SETTINGS.showSpend).toBe(false); // expenditures hidden by default
    expect(DEFAULT_SETTINGS.dark).toBe(false);
    expect(DEFAULT_SETTINGS.env).toBe("");
    expect(DEFAULT_SETTINGS.customEnvs).toEqual([]);
    expect(DEFAULT_SETTINGS.hiddenEnvs).toEqual([]);
  });
});

describe("loadSettings", () => {
  it("returns defaults when nothing is stored", () => {
    expect(loadSettings()).toEqual(DEFAULT_SETTINGS);
  });

  it("merges stored values over defaults (forward-compatible)", () => {
    localStorage.setItem(KEY, JSON.stringify({ plan: "team", dark: true }));
    const s = loadSettings();
    expect(s.plan).toBe("team");
    expect(s.dark).toBe(true);
    expect(s.showSpend).toBe(false); // missing key falls back to default
    expect(s.autoPlan).toBe(true);
  });

  it("falls back to defaults on corrupt JSON", () => {
    localStorage.setItem(KEY, "{not valid json");
    expect(loadSettings()).toEqual(DEFAULT_SETTINGS);
  });
});

describe("saveSettings", () => {
  it("round-trips through localStorage", () => {
    saveSettings({ ...DEFAULT_SETTINGS, seats: 12, accent: "#000000", showSpend: true });
    const s = loadSettings();
    expect(s.seats).toBe(12);
    expect(s.accent).toBe("#000000");
    expect(s.showSpend).toBe(true);
  });
});

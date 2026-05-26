import type { Settings } from "./types";

const KEY = "claude-usage.settings";

export const DEFAULT_SETTINGS: Settings = {
  env: "",
  customEnvs: [],
  hiddenEnvs: [],
  plan: "max5",
  autoPlan: true,
  seats: 5,
  showSpend: false,
  dark: false,
  compact: false,
  accent: "#D97757",
};

export function loadSettings(): Settings {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return DEFAULT_SETTINGS;
    return { ...DEFAULT_SETTINGS, ...JSON.parse(raw) };
  } catch {
    return DEFAULT_SETTINGS;
  }
}

export function saveSettings(s: Settings) {
  try {
    localStorage.setItem(KEY, JSON.stringify(s));
  } catch {
    /* ignore quota / private-mode errors */
  }
}

export const ACCENT_SWATCHES = ["#D97757", "#C45F3F", "#C68B36", "#6B8E5A", "#5A7CA8"];

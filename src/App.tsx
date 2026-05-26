import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { enable as enableAutostart, disable as disableAutostart, isEnabled as isAutostartEnabled } from "@tauri-apps/plugin-autostart";
import { Widget } from "./components/Widget";
import { SettingsPanel } from "./components/SettingsPanel";
import { useUsage } from "./hooks/useUsage";
import { useRealUsage } from "./hooks/useRealUsage";
import { useEnvironments } from "./hooks/useEnvironments";
import { PLANS } from "./lib/plans";
import { DEFAULT_SETTINGS, loadSettings, saveSettings } from "./lib/settings";
import type { PlanTier, Settings } from "./lib/types";

export default function App() {
  const [settings, setSettings] = useState<Settings>(() => loadSettings() ?? DEFAULT_SETTINGS);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [autostart, setAutostart] = useState(false);

  // Reflect the OS-level launch-at-login registration.
  useEffect(() => {
    isAutostartEnabled().then(setAutostart).catch(() => {});
  }, []);
  const toggleAutostart = async () => {
    try {
      if (autostart) {
        await disableAutostart();
        setAutostart(false);
      } else {
        await enableAutostart();
        setAutostart(true);
      }
    } catch {
      /* not in Tauri / unsupported */
    }
  };

  const { environments } = useEnvironments(settings.customEnvs);

  // Hide removed accounts from the switcher (discovered dirs are hidden, not deleted).
  const visibleEnvs = environments.filter((e) => !settings.hiddenEnvs.includes(e.id));

  // Selected environment: explicit choice (if still visible), else default `.claude`, else first.
  const selectedEnvId =
    (settings.env && visibleEnvs.some((e) => e.id === settings.env) ? settings.env : "") ||
    visibleEnvs.find((e) => e.id === ".claude")?.id ||
    visibleEnvs[0]?.id ||
    "";
  const selectedEnv = visibleEnvs.find((e) => e.id === selectedEnvId);
  const account = selectedEnv?.account;

  const { data, refresh } = useUsage(selectedEnvId);
  const { real, refresh: refreshReal } = useRealUsage(selectedEnvId);

  // Plan: auto-detected from the selected account, or manual override.
  const detected = account?.detectedPlan || "";
  const planTier: PlanTier = settings.autoPlan && detected ? (detected as PlanTier) : settings.plan;
  const plan = PLANS[planTier] ?? PLANS.max5;

  // Apply dark class + accent CSS var, persist on change.
  useEffect(() => {
    document.documentElement.classList.toggle("dark", settings.dark);
    document.documentElement.style.setProperty("--clay", settings.accent);
    saveSettings(settings);
  }, [settings]);

  // Match the native window backdrop (Mica/vibrancy) to the theme so a light
  // UI doesn't pick up a dark Mica tint on dark-themed systems.
  useEffect(() => {
    invoke("set_window_theme", { dark: settings.dark }).catch(() => {});
  }, [settings.dark]);

  const setSetting = useMemo(
    () =>
      <K extends keyof Settings>(key: K, value: Settings[K]) =>
        setSettings((prev) => ({ ...prev, [key]: value })),
    []
  );

  return (
    <>
      <Widget
        data={data}
        real={real}
        plan={plan}
        seats={settings.seats}
        compact={settings.compact}
        showSpend={settings.showSpend}
        account={account}
        envLabel={selectedEnv?.label ?? ""}
        onOpenSettings={() => setSettingsOpen(true)}
        onRefresh={() => {
          refresh();
          refreshReal();
        }}
      />
      {settingsOpen && (
        <SettingsPanel
          settings={settings}
          environments={visibleEnvs}
          selectedEnvId={selectedEnvId}
          detectedPlan={detected}
          autostart={autostart}
          onToggleAutostart={toggleAutostart}
          onChange={setSetting}
          onClose={() => setSettingsOpen(false)}
        />
      )}
    </>
  );
}

import { useState } from "react";
import { Icon } from "./Icon";
import { ACCENT_SWATCHES } from "../lib/settings";
import { PLANS } from "../lib/plans";
import type { Environment, PlanTier, Settings } from "../lib/types";

const PLAN_OPTIONS: { value: PlanTier; label: string }[] = [
  { value: "pro", label: "Pro" },
  { value: "max5", label: "Max 5×" },
  { value: "max20", label: "Max 20×" },
  { value: "team", label: "Team" },
];

const PLAN_LABEL: Record<string, string> = {
  pro: "Pro",
  max5: "Max 5×",
  max20: "Max 20×",
  team: "Team",
};

export function SettingsPanel({
  settings,
  environments,
  selectedEnvId,
  detectedPlan,
  autostart,
  onToggleAutostart,
  onChange,
  onClose,
}: {
  settings: Settings;
  environments: Environment[];
  selectedEnvId: string;
  detectedPlan: string;
  autostart: boolean;
  onToggleAutostart: () => void;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
  onClose: () => void;
}) {
  const [newEnv, setNewEnv] = useState("");

  // Effective plan tier currently driving the widget.
  const effectivePlan: PlanTier =
    settings.autoPlan && detectedPlan ? (detectedPlan as PlanTier) : settings.plan;

  const addEnv = () => {
    const p = newEnv.trim();
    if (!p || settings.customEnvs.includes(p)) return;
    onChange("customEnvs", [...settings.customEnvs, p]);
    onChange("env", p);
    setNewEnv("");
  };
  // Remove an account: drop user-added paths entirely; hide discovered dirs
  // (their config dirs are left untouched and can be restored).
  const removeEnv = (env: Environment) => {
    if (env.custom) {
      onChange("customEnvs", settings.customEnvs.filter((p) => p !== env.id));
    } else if (!settings.hiddenEnvs.includes(env.id)) {
      onChange("hiddenEnvs", [...settings.hiddenEnvs, env.id]);
    }
    if (settings.env === env.id) onChange("env", "");
  };
  const restoreHidden = () => onChange("hiddenEnvs", []);

  return (
    <div className="sp-overlay" onClick={onClose}>
      <div className="sp-panel" onClick={(e) => e.stopPropagation()}>
        <div className="sp-hd">
          <b>Settings</b>
          <button className="sp-x" onClick={onClose} title="Close"><Icon name="x" size={13} /></button>
        </div>
        <div className="sp-body">
          {/* ── Account / environment ── */}
          <div className="sp-sect">Account</div>
          <div className="sp-env-list">
            {environments.length === 0 && (
              <div className="sp-env-empty">No Claude environments found.</div>
            )}
            {environments.map((e) => (
              <button
                key={e.id}
                className={`sp-env ${e.id === selectedEnvId ? "on" : ""}`}
                onClick={() => onChange("env", e.id)}
              >
                <span className="sp-env-main">
                  <span className="sp-env-name">{e.label}</span>
                  <span className="sp-env-meta">
                    {e.account.email || e.id}
                    {e.custom ? " · custom" : ""}
                  </span>
                </span>
                {e.account.detectedPlan && (
                  <span className="sp-env-badge">{PLAN_LABEL[e.account.detectedPlan] ?? e.account.detectedPlan}</span>
                )}
                <span
                  className="sp-env-rm"
                  title={e.custom ? "Remove" : "Hide account"}
                  onClick={(ev) => {
                    ev.stopPropagation();
                    removeEnv(e);
                  }}
                >
                  <Icon name="x" size={11} />
                </span>
              </button>
            ))}
            {settings.hiddenEnvs.length > 0 && (
              <button className="sp-env-restore" onClick={restoreHidden}>
                Restore {settings.hiddenEnvs.length} hidden account{settings.hiddenEnvs.length > 1 ? "s" : ""}
              </button>
            )}
          </div>
          <div className="sp-add">
            <input
              className="sp-add-input"
              placeholder="Add config dir path…"
              value={newEnv}
              onChange={(ev) => setNewEnv(ev.target.value)}
              onKeyDown={(ev) => ev.key === "Enter" && addEnv()}
            />
            <button className="sp-add-btn" onClick={addEnv}>Add</button>
          </div>

          {/* ── Plan ── */}
          <div className="sp-sect">Plan</div>
          <div className="sp-row">
            <span className="sp-lbl">Auto-detect</span>
            <button className="sp-toggle" data-on={settings.autoPlan ? 1 : 0} onClick={() => onChange("autoPlan", !settings.autoPlan)}>
              <i />
            </button>
          </div>
          {settings.autoPlan ? (
            <div className="sp-detected">
              {detectedPlan ? (
                <>Detected: <b>{PLAN_LABEL[detectedPlan] ?? detectedPlan}</b></>
              ) : (
                <>No plan detected for this account — using <b>{PLAN_LABEL[settings.plan]}</b></>
              )}
            </div>
          ) : (
            <div className="sp-seg">
              {PLAN_OPTIONS.map((o) => (
                <button
                  key={o.value}
                  className={settings.plan === o.value ? "on" : ""}
                  onClick={() => onChange("plan", o.value)}
                >
                  {o.label}
                </button>
              ))}
            </div>
          )}
          {effectivePlan === "team" && (
            <div className="sp-row">
              <span className="sp-lbl">Seats</span>
              <div className="sp-stepper">
                <button onClick={() => onChange("seats", Math.max(PLANS.team.minSeats ?? 1, settings.seats - 1))} title="Fewer seats">−</button>
                <span className="sp-stepper-val mono">{settings.seats}</span>
                <button onClick={() => onChange("seats", settings.seats + 1)} title="More seats">+</button>
              </div>
            </div>
          )}

          {/* ── Appearance ── */}
          <div className="sp-sect">Appearance</div>
          <div className="sp-row">
            <span className="sp-lbl">Dark mode</span>
            <button className="sp-toggle" data-on={settings.dark ? 1 : 0} onClick={() => onChange("dark", !settings.dark)}>
              <i />
            </button>
          </div>
          <div className="sp-row">
            <span className="sp-lbl">Compact view</span>
            <button className="sp-toggle" data-on={settings.compact ? 1 : 0} onClick={() => onChange("compact", !settings.compact)}>
              <i />
            </button>
          </div>
          <div className="sp-row">
            <span className="sp-lbl">Show expenditures</span>
            <button className="sp-toggle" data-on={settings.showSpend ? 1 : 0} onClick={() => onChange("showSpend", !settings.showSpend)}>
              <i />
            </button>
          </div>

          <div className="sp-sect">Startup</div>
          <div className="sp-row">
            <span className="sp-lbl">Launch at startup</span>
            <button className="sp-toggle" data-on={autostart ? 1 : 0} onClick={onToggleAutostart}>
              <i />
            </button>
          </div>

          <div className="sp-sect">Accent</div>
          <div className="sp-swatches">
            {ACCENT_SWATCHES.map((c) => (
              <button
                key={c}
                className="sp-swatch"
                data-on={settings.accent === c ? 1 : 0}
                style={{ background: c, color: c }}
                onClick={() => onChange("accent", c)}
                title={c}
              />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

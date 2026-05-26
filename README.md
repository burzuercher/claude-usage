# Claude Usage

An always-on-top desktop widget that shows your **real** Claude usage, built with
[Tauri](https://tauri.app) + React. It reads the local Claude Code / Claude Desktop
transcript logs in `~/.claude/projects/**/*.jsonl` (the same source as
[`ccusage`](https://github.com/ryoppippi/ccusage)) and aggregates them into a warm,
Claude-themed floating window.

Implemented from a [Claude Design](https://claude.ai/design) handoff (`Claude Usage Widget.html`).

## Features

- **Frameless, translucent (Mica) always-on-top window** — Windows 11 style title bar with a
  pin (always-on-top) toggle and min / max / close. Drag by the title bar.
- **Overview** — animated 5-hour session ring (color ramps clay → amber → ember as it fills),
  a pace forecast, weekly bars (all models + Opus-only), the session model split, and a
  burn-rate sparkline with a "safe pace" line.
- **Models** — per-model cards (Opus / Sonnet / Haiku) with token totals and estimated cost.
- **Surfaces** — Claude Code vs Claude Desktop split + a 7-day stacked daily chart.
  (claude.ai web chat isn't logged locally, so it isn't shown.)
- **History** — recent sessions (labeled by their first prompt) and a 7×24 activity heatmap.
- **Footer** — today's tokens, estimated cost, and prompt count.
- **Spend** — month-to-date expenditure: subscription base + estimated extra-usage (overage past
  plan limits, billed at API rates) + projected month-end total, with a monthly-allowance bar.
- **Settings** (gear icon):
  - **Account switcher** — auto-discovers every Claude environment (separate `~/.claude*` config
    dirs, e.g. `~/.claude`, `~/.claude-max`, `~/.claude-team`), shows each one's org / email /
    detected plan, and lets you **toggle between accounts**. **Add a custom config-dir path** manually,
    and **remove any account** from the list (custom paths are dropped; discovered dirs are hidden,
    not deleted, and restorable via "Restore N hidden").
  - **Auto-detect plan** — reads the selected account's `oauthAccount` and sets the plan
    automatically (Pro / Max 5× / Max 20× / Team). Turn it off to choose manually.
  - **Team seats**, dark mode, compact view, **show/hide expenditures** (hidden by default),
    **launch at startup** (OS autostart), and accent color. Persisted to `localStorage`
    (autostart is an OS-level registration).

## Live usage (matches the Claude app)

When the selected account has a valid Claude Code OAuth token, the widget calls Anthropic's
**`GET /api/oauth/usage`** — the same source the Claude app's "Your usage limits" page uses — and shows
**authoritative** numbers: real session %, the weekly categories that apply to your plan (All models /
Sonnet only / Claude Design / Opus only), real reset times (e.g. "resets Fri 2:00 PM"), and real
**extra-usage** spend (`extra_usage.used_credits`, plus enabled state and monthly cap). A green **LIVE**
badge appears, and the session forecast is projected from the real utilization.

The token is read **at runtime** from the selected environment's `.credentials.json` and used only if
unexpired — the widget never refreshes/rotates it (which could disrupt Claude Code's own login) and
never logs or transmits it anywhere except Anthropic's own API. If no valid token is present, the widget
falls back to the local-log estimate (and says so).

## Data & accuracy

- Token counts, cost, model split, daily/history, and the 5-hour block are computed **for real**
  from local logs. Cost uses the per-model price table in `src-tauri/src/pricing.rs`
  (Opus 4.7 = $5/$25 per Mtok, verified against ccusage) — easy to update if prices change.
- Anthropic exposes **no public API** for Pro/Max session or weekly *limits*, so the progress-bar
  percentages divide real prompt counts by the **estimated** plan budgets in `src/lib/plans.ts`.
  Pick your plan tier in Settings.
- If no logs are found, the widget shows bundled demo data (a `demo` badge appears in the title).

## Develop

```bash
npm install
npm run tauri dev      # launches the widget
```

## Build

```bash
npm run tauri build    # produces an installer / executable under src-tauri/target
```

Outputs (Windows): `src-tauri/target/release/claude-usage.exe` (~10 MB) plus an `.msi` and an
NSIS `-setup.exe` under `src-tauri/target/release/bundle/`.

## Test

```bash
npm test                       # frontend unit tests (Vitest): format, economics, settings
cargo test --manifest-path src-tauri/Cargo.toml   # backend tests: pricing, usage aggregation,
                                                  # env resolution, account detection, usage-API parsing
```

Backend tests are hermetic — usage aggregation runs against fixtures written to a temp dir (no
dependency on your real `~/.claude` logs).

## Project layout

- `src/` — React/TypeScript frontend (ported pixel-for-pixel from the design prototype)
  - `components/` — `Widget`, `Ring`, `StackedBar`, `BurnSpark`, `Icon`, `SettingsPanel`, `tabs/`
  - `hooks/` — `useUsage` & `useRealUsage` (poll the backend every 30/60s), `useEnvironments`, `useCountUp`
  - `lib/` — `types`, `plans` (budgets + `computeEconomics`), `format`, `mockData`, `settings`
- `src-tauri/src/` — Rust backend
  - `usage.rs` — walks & parses the logs, de-dups on `(requestId, message.id)`, aggregates
    (session / weekly / month / models / surfaces / burn / daily / recent / heatmap)
  - `pricing.rs` — per-model token pricing
  - `account.rs` — reads `oauthAccount` from a `.claude.json` (email / org / tier → detected plan)
  - `env.rs` — discovers `~/.claude*` environments and resolves a chosen one (or a custom path)
  - `realusage.rs` — fetches authoritative limits/extra-usage from `GET /api/oauth/usage` (runtime token)
  - `lib.rs` — `get_usage` / `get_account` / `get_real_usage` / `list_environments` / `set_window_theme`
    commands + theme-matched Mica/vibrancy setup

### Multi-account notes

Each account lives in its own config dir with its own `projects/` logs and `.claude.json`. The widget
lists them and aggregates whichever you pick. Because the per-message logs are **not** tagged with an
account, usage can't be split across accounts *within a single dir* — but separate dirs
(`~/.claude-max`, `~/.claude-team`, …) are fully distinguishable, which is the normal multi-account
setup. Plan budgets and subscription prices are estimates in `src/lib/plans.ts` (no public API exposes
them); auto-detection maps `organizationType` / `userRateLimitTier` to a plan tier.

The widget polls `get_usage` every 30 seconds; numbers animate on update.

> Note: Mica translucency is Windows-only. macOS uses native vibrancy; on Linux the solid warm
> background shows instead.

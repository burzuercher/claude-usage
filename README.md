# Claude Usage

An always-on-top desktop widget that shows your **real** Claude usage, built with
[Tauri](https://tauri.app) + React. Usage *limits* (session ring, weekly bars, extra-usage
spend) come from Anthropic's own usage endpoint — the same numbers the Claude app shows — and the
factual local data (tokens, estimated cost, per-model split, burn rate) is aggregated from the
Claude Code / Claude Desktop transcript logs in `~/.claude*/projects/**/*.jsonl` (the same source
as [`ccusage`](https://github.com/ryoppippi/ccusage)).

Implemented from a [Claude Design](https://claude.ai/design) handoff (`Claude Usage Widget.html`).

## Features

- **Frameless, translucent (Mica) always-on-top window** — Windows 11 style title bar with a
  pin (always-on-top) toggle and min / max / close. Drag by the title bar.
- **Overview** — animated 5-hour session ring (color ramps clay → amber → ember
  as it fills), a pace forecast, weekly bars for every window Anthropic reports (All models,
  Fable only, Opus only, Sonnet only, …) with any active **limit-boost promo** noted under the
  bar, the **current session's** model split (Fable / Opus / Sonnet / Haiku), a **session burn
  chart** (tokens per 15 min from window start to reset, with a "safe pace" line = the rate that
  lands exactly at 100% at reset), and an optional expenditure card (subscription + real
  extra-usage spend + month-end projection). The **pace forecast** projects the last 30 minutes of
  burn — calibrated to the live % via the window's own token total — to a time-to-cap or a
  projected % at reset (falling back to the session's average pace when there's too little data).
- **Limits** — the other half of Claude Code's own `/usage` screen, which the usage API doesn't
  serve: **what's contributing to your limits usage**. Overlapping behaviors (usage at >150k
  context, >100k-token cache misses, subagent-heavy sessions, 4+ sessions in parallel, sessions
  active 8+ hours) with the same guidance copy the CLI prints, plus **% of usage by Skill,
  Subagent, MCP server and plugin**. Toggle between the **last 24h and last 7d**. Derived locally
  and machine-local, exactly as the CLI's screen is — hover a behavior for its request/session count.
- **Sessions** — every session with activity in the last 7 days, pick one to see its **real cost**
  (Claude Code's own `cost-state` record, not an estimate), turns, wall and API time, tool time,
  lines added/removed, the input / output / cache-read / cache-write split, and the models used.
  A session Claude Code hasn't written a cost record for yet shows `—` rather than a guess.
- **Footer** — today's tokens, **real extra-usage spend this month** (from Anthropic, not an estimate), and prompt count.
- **Title bar** — settings, refresh, pin (always-on-top) and the usual window controls.
- **Tabs** — Overview / Limits / Sessions. The two local-attribution views only scan
  transcripts while one of them is on screen.
- **Settings** (sliders icon):
  - **Account switcher** — auto-discovers every Claude environment (separate `~/.claude*` config
    dirs, e.g. `~/.claude`, `~/.claude-max`, `~/.claude-team`), shows each one's org / email /
    detected plan, and lets you **toggle between accounts**. **Add a custom config-dir path** manually,
    and **remove any account** from the list (custom paths are dropped; discovered dirs are hidden,
    not deleted, and restorable via "Restore N hidden").
  - **Auto-detect plan** — reads the selected account's `oauthAccount` and sets the plan
    automatically (Pro / Max 5× / Max 20× / Team). Turn it off to choose manually.
  - **Team seats**, dark mode, compact view, **show/hide the expenditure card** (hidden by
    default), **launch at startup** (OS autostart), and accent color. Persisted to `localStorage`
    (autostart is an OS-level registration).

## Live usage (matches the Claude app)

When the selected account has a valid Claude Code OAuth token, the widget calls Anthropic's
**`GET /api/oauth/usage`** — the same source the Claude app's "Your usage limits" page and Claude
Code's `/usage` use — and shows **authoritative** numbers:

- the session ring and every weekly window from the response's normalized **`limits[]`** array
  (`session`, `weekly_all`, and per-model `weekly_scoped` entries such as **Fable**), with real reset
  times ("resets Fri 2:00 PM") and the server's `is_active` hint colouring the binding bar. Legacy
  top-level keys (`seven_day_opus`, `seven_day_sonnet`, …) are merged in for older responses.
- real **extra-usage** spend from the `spend` block (`amount_minor` / `extra_usage.used_credits` are
  in **cents**; the widget converts), plus enabled state, cap, and disabled reason.
- **Temporary limit boosts.** Anthropic announces promos such as "+50% weekly limits promo through
  Sep 13" via a feature flag that Claude Code caches in the account file
  (`.claude.json → cachedGrowthBookFeatures.tengu_rate_limit_promo_notices`). The boost itself is already
  reflected in the server's utilization %, so the widget just shows the notice under the matching bar
  (and drops it once its "through <date>" has passed). Grace-window state only travels on message
  response headers and is not observable here.

A green **live** badge appears and the session forecast is projected from the real utilization.

**Rate limiting & staleness.** The endpoint accepts **one request per token per 2-minute window**
(measured: successes exactly 2 min apart, everything in between a 429) and that budget is shared with
Claude Code itself. The widget therefore polls just over every 2 minutes, retries ~1 minute after a
collision (backing off to 5 min on repeated ones, honouring `Retry-After`), and shows "updated Xm ago"
under the bars — amber and marked *stale* after 10 minutes. As a second source it
reads **Claude Code's own cached response** (`.claude.json → cachedUsageUtilization`, refreshed by the
CLI at most every 5 minutes): a fresh cache (< 2 min) is served without a network call, and one up to
an hour old is the fallback when the API fails, shown with a **cached** badge and an "updated Xm ago"
line instead of blanking the ring. Older caches are ignored, as Claude Code itself does.

The token is read **at runtime** from the selected environment's `.credentials.json` and used only if
unexpired — the widget never refreshes/rotates it (which could disrupt Claude Code's own login) and
never logs or transmits it anywhere except Anthropic's own API. If neither a valid token nor a cached
response is available, the ring is replaced by an explanation of why.

## Data & accuracy

- Token counts, estimated cost, the per-model split, and the burn sparkline are computed **for real**
  from local logs. The model split covers the **current session window** (the live reset time − 5h
  when available, else a ccusage-style local 5-hour block). Cost uses the version-aware price table in
  `src-tauri/src/pricing.rs` — Fable 5.1 $10/$50, Opus 5 $5/$25, Sonnet 5 $2/$10, Haiku 4.5 $1/$5 per
  MTok, with 5-minute (1.25×) vs 1-hour (2×) cache writes priced separately — easy to update if prices
  change.
- The estimated cost is what the same tokens would cost at API list prices — a reference figure, not
  what a subscription bills.
- The **Limits** tab re-implements Claude Code 2.1.259's own attribution: the same fields
  (`isSidechain`, `attributionAgent` / `Skill` / `Plugin` / `McpServer`), the same thresholds
  (>100k uncached, >150k prompt, 3 subagent turns or >50% subagent cost, 4 sessions per 5-minute
  bucket, 8 distinct active hours, a 10% floor for reporting) and the same relative weighting
  (cache read 1x, uncached input 10x, cache write 12.5x, output 50x, scaled by model tier
  Fable 10 / Opus 5 / Sonnet 3 / Haiku 1). That weight is a synthetic unit used only for
  percentages — it is never shown as dollars. Verified side by side against `/usage`.
- Attribution and the session list read only transcripts touched in the last 7 days, so the wider
  window costs less to scan than the Overview aggregation does.
- The **session cost** on the Sessions tab is Claude Code's own `totalCostUSD`, so it reflects real
  billing rather than the price table below.
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

Outputs (macOS): `src-tauri/target/release/bundle/macos/Claude Usage.app` plus a `.dmg` under
`src-tauri/target/release/bundle/dmg/`.

### macOS

Prerequisites: the Xcode Command Line Tools (`xcode-select --install`), a Rust toolchain
(`rustup`, or Homebrew's `rust`), and Node. Then the standard flow works unchanged:

```bash
npm install
npm run tauri dev      # launches the widget
npm run tauri build    # produces Claude Usage.app + a .dmg
```

The widget lives in the **macOS menu bar** (the tray icons at the top-right of the screen):

- **Left-click** the menu-bar icon to show/hide the widget.
- **Right-click** for a menu: *Show Claude Usage*, *Hide*, *Quit*.
- **Closing** the widget window hides it to the menu bar rather than quitting — reopen it from the
  icon, or choose *Quit* to exit fully.

Translucency uses native macOS vibrancy (Mica is Windows-only). "Launch at startup" in Settings
registers a macOS `LaunchAgent`.

### Linux build (from a Windows host, via Docker)

A `Dockerfile.linux-build` ships a `rust:1-bookworm` image with Tauri's Linux deps (webkit2gtk 4.1,
gtk3, ayatana-appindicator, rsvg, xdo, ssl, build-essential, Node 22). It bind-mounts the project and
writes Linux artifacts to a separate `src-tauri/target-linux/` so it never conflicts with the Windows
`target/`.

```bash
docker build -t claude-usage-linux-build -f Dockerfile.linux-build .
docker run --rm \
  -v "$PWD:/work" \
  -v claude-usage-node-modules:/work/node_modules \
  claude-usage-linux-build
```

The named volume keeps Linux `node_modules` isolated from the host's Windows shims.

Outputs under `src-tauri/target-linux/release/`:
- `claude-usage` (standalone binary, ~9 MB)
- `bundle/deb/Claude Usage_0.1.0_amd64.deb` (~3.5 MB)
- `bundle/rpm/Claude Usage-0.1.0-1.x86_64.rpm` (~3.5 MB)
- `bundle/appimage/Claude Usage_0.1.0_amd64.AppImage` (~94 MB — bundles the WebKit runtime)

## Test

```bash
npm test                       # frontend unit tests (Vitest): format, economics, settings
cargo test --manifest-path src-tauri/Cargo.toml   # backend tests: pricing, usage aggregation,
                                                  # env resolution, account detection, usage-API
                                                  # parsing (limits[], spend, promo notes, cache)
CLAUDE_USAGE_SMOKE_ENV=.claude-team cargo test --manifest-path src-tauri/Cargo.toml -- --ignored --nocapture
                                                  # manual smoke tests: real fetch, log aggregation, and the
                                                  # attribution numbers for one env (diff against /usage)
```

Backend tests are hermetic — usage aggregation runs against fixtures written to a temp dir (no
dependency on your real `~/.claude` logs).

## Project layout

- `src/` — React/TypeScript frontend (ported from the design prototype)
  - `components/` — `Widget`, `Overview`, `Limits`, `Sessions`, `Ring`, `StackedBar`, `BurnSpark`,
    `Icon`, `SettingsPanel`
  - `hooks/` — `useUsage` (polls local logs every 30s) & `useRealUsage` (live limits every 2 min with
    429 backoff), `useAttribution` (local attribution every 60s, only while its tab is open),
    `useEnvironments`, `useCountUp`
  - `lib/` — `types`, `plans` (subscription prices + `computeEconomics`), `format`, `mockData`, `settings`
- `src-tauri/src/` — Rust backend
  - `usage.rs` — walks & parses the logs, de-dups on `(requestId, message.id)`, aggregates
    (session window / today / month / per-model split / burn)
  - `attribution.rs` — the `/usage` attribution algorithm (behaviors + skill / subagent / MCP /
    plugin shares over 24h and 7d) and per-session accounting from `cost-state` records
  - `pricing.rs` — version-aware per-model token pricing (Fable / Opus / Sonnet / Haiku)
  - `account.rs` — reads `oauthAccount` from a `.claude.json` (email / org / tier → detected plan)
  - `env.rs` — discovers `~/.claude*` environments and resolves a chosen one (or a custom path)
  - `realusage.rs` — authoritative limits / extra-usage from `GET /api/oauth/usage` (runtime token),
    Claude Code's cached copy as fallback, and promo-notice flags
  - `lib.rs` — `get_usage` / `get_attribution` / `get_account` / `get_real_usage` /
    `list_environments` / `set_window_theme`
    commands + theme-matched Mica/vibrancy setup

### Multi-account notes

Each account lives in its own config dir with its own `projects/` logs and `.claude.json`. The widget
lists them and aggregates whichever you pick. Because the per-message logs are **not** tagged with an
account, usage can't be split across accounts *within a single dir* — but separate dirs
(`~/.claude-max`, `~/.claude-team`, …) are fully distinguishable, which is the normal multi-account
setup. Plan budgets and subscription prices are estimates in `src/lib/plans.ts` (no public API exposes
them); auto-detection maps `organizationType` / `userRateLimitTier` to a plan tier.

The widget polls `get_usage` every 30 seconds and the live limits every 2 minutes; numbers animate on update.

> Note: Mica translucency is Windows-only. macOS uses native vibrancy; on Linux the solid warm
> background shows instead.

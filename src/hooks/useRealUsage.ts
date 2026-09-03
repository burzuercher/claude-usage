import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { RealUsage } from "../lib/types";

/** Normal poll cadence. Measured behaviour of /api/oauth/usage: exactly one
 *  successful request per token per 2-minute window, everything else 429 — so
 *  poll just over that window. */
const BASE_MS = 125_000;
/** After a 429 (someone else — usually Claude Code — used this window's
 *  request), retry about a minute later so we land in the next window at a
 *  different phase; repeated collisions back off up to 5 minutes. */
const COLLISION_RETRY_MS = 65_000;
const MAX_BACKOFF_MS = 5 * 60_000;

const NONE: RealUsage = {
  found: false,
  reason: "",
  source: "",
  session: null,
  weekly: [],
  extra: null,
  fetchedAt: 0,
  httpStatus: 0,
  retryAfterSecs: 0,
  promoCachedAt: 0,
};

interface UseRealUsage {
  real: RealUsage;
  /** True while the first call for the current env is still in flight. */
  loading: boolean;
  refresh: () => void;
}

// Fetches authoritative usage limits from Anthropic for the given environment.
//
// Correctness properties:
//  1. Stale-result protection: when the user switches envs, an in-flight call
//     for the old env must not poison the new env's state. Each env gets its
//     own effect scope with a `cancelled` flag; results after cancellation are
//     dropped.
//  2. Stickiness: once we've seen live data for an env, a later found=false
//     poll never replaces it (network blips shouldn't make the live ring blink
//     into an empty state). Only the very first call for an env may surface a
//     not-found result. (The backend already degrades to Claude Code's cached
//     response when the API fails, so found=false is now rare.)
//  3. Backoff: a 429 retries ~65s later (next window, different phase), then
//     doubles on repeated collisions up to 5 min, honouring Retry-After when
//     larger; a successful API fetch resets it. Results served from Claude
//     Code's cache neither reset nor grow the delay.
export function useRealUsage(envId: string): UseRealUsage {
  const [real, setReal] = useState<RealUsage>(NONE);
  const [loading, setLoading] = useState(true);
  const refreshRef = useRef<() => void>(() => {});

  useEffect(() => {
    let cancelled = false;
    let seenLive = false;
    let collisions = 0;
    let timer: number | undefined;
    setReal(NONE);
    setLoading(true);

    const run = async (first: boolean) => {
      let next = BASE_MS;
      try {
        const res = await invoke<RealUsage>("get_real_usage", { envId });
        if (cancelled) return;
        if (res.found) {
          seenLive = true;
          setReal(res);
        } else if (!seenLive) {
          // First call for this env returned not-found — surface so the UI can
          // show its empty state with the specific reason.
          setReal(res);
        } else {
          console.warn("[real-usage] poll returned not-found, keeping last value:", res.reason);
        }

        if (res.httpStatus === 429) {
          collisions += 1;
          const backoff = Math.min(MAX_BACKOFF_MS, COLLISION_RETRY_MS * 2 ** (collisions - 1));
          next = Math.max(backoff, (res.retryAfterSecs || 0) * 1000);
        } else if (res.source === "api") {
          collisions = 0;
        }
      } catch (e) {
        if (cancelled) return;
        if (!seenLive) setReal({ ...NONE, reason: "not available" });
        else console.warn("[real-usage] poll threw, keeping last value:", e);
        next = BASE_MS;
      } finally {
        if (!cancelled && first) setLoading(false);
      }
      if (!cancelled) timer = window.setTimeout(() => void run(false), next);
    };

    refreshRef.current = () => {
      if (cancelled) return;
      window.clearTimeout(timer);
      void run(false);
    };
    void run(true);

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
      refreshRef.current = () => {};
    };
  }, [envId]);

  const refresh = useCallback(() => refreshRef.current(), []);

  return { real, loading, refresh };
}

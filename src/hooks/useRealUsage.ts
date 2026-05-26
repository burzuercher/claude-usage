import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { RealUsage } from "../lib/types";

const POLL_MS = 60_000;

const NONE: RealUsage = {
  found: false,
  reason: "",
  session: null,
  weekly: [],
  extra: null,
  fetchedAt: 0,
};

// Fetches authoritative usage limits from Anthropic for the given environment.
// Returns found=false (with a reason) when unavailable, so the UI can fall back
// to the local-log estimate.
//
// Stickiness: once we've seen live data for an env, we never replace it with a
// found=false response. A transient API blip (network, slow response, token
// hiccup) used to silently swap in a wrong local estimate (e.g. 99% when the
// real session was 64%). Now we keep the last good value until a *successful*
// fetch supersedes it; only the very first call for an env is allowed to
// surface a not-found result.
export function useRealUsage(envId: string): { real: RealUsage; refresh: () => void } {
  const [real, setReal] = useState<RealUsage>(NONE);
  const mounted = useRef(true);
  // Track whether we've ever seen a successful response for this env. Reset on
  // env change so the new env starts fresh.
  const seenLive = useRef(false);

  const refresh = useCallback(async () => {
    try {
      const res = await invoke<RealUsage>("get_real_usage", { envId });
      if (!mounted.current) return;
      if (res.found) {
        seenLive.current = true;
        setReal(res);
      } else if (!seenLive.current) {
        // No successful read yet for this env — surface the not-found so the
        // UI can fall back to the local estimate.
        setReal(res);
      } else {
        // Already had live data; this poll failed — keep the last good value.
        // Log so the cause is debuggable if it's persistent.
        console.warn("[real-usage] poll returned not-found, keeping last value:", res.reason);
      }
    } catch (e) {
      if (!mounted.current) return;
      if (!seenLive.current) setReal({ ...NONE, reason: "not available" });
      else console.warn("[real-usage] poll threw, keeping last value:", e);
    }
  }, [envId]);

  useEffect(() => {
    mounted.current = true;
    // Reset state when the env changes so the new env can't inherit the old
    // env's sticky value.
    seenLive.current = false;
    setReal(NONE);
    refresh();
    const id = setInterval(refresh, POLL_MS);
    return () => {
      mounted.current = false;
      clearInterval(id);
    };
  }, [refresh]);

  return { real, refresh };
}

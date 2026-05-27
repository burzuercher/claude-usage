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

interface UseRealUsage {
  real: RealUsage;
  /** True while the first call for the current env is still in flight. */
  loading: boolean;
  refresh: () => void;
}

// Fetches authoritative usage limits from Anthropic for the given environment.
//
// Two correctness properties:
//  1. Stale-result protection: when the user switches envs, an in-flight call
//     for the old env must not poison the new env's state. We capture the
//     envId at call time and compare against `activeEnvId.current` on
//     resolution; mismatches are dropped.
//  2. Stickiness: once we've seen live data for an env, a later found=false
//     poll never replaces it (network blips shouldn't make the live ring blink
//     into an empty state). Only the very first call for an env may surface a
//     not-found result.
export function useRealUsage(envId: string): UseRealUsage {
  const [real, setReal] = useState<RealUsage>(NONE);
  const [loading, setLoading] = useState(true);
  const mounted = useRef(true);
  const seenLive = useRef(false);
  // Latest envId, used to discard in-flight results for a since-switched env.
  const activeEnvId = useRef(envId);
  activeEnvId.current = envId;

  const refresh = useCallback(async () => {
    const callEnvId = envId;
    try {
      const res = await invoke<RealUsage>("get_real_usage", { envId: callEnvId });
      if (!mounted.current || activeEnvId.current !== callEnvId) return;
      if (res.found) {
        seenLive.current = true;
        setReal(res);
      } else if (!seenLive.current) {
        // First call for this env returned not-found — surface so the UI can
        // show its empty state with the specific reason.
        setReal(res);
      } else {
        console.warn("[real-usage] poll returned not-found, keeping last value:", res.reason);
      }
    } catch (e) {
      if (!mounted.current || activeEnvId.current !== callEnvId) return;
      if (!seenLive.current) setReal({ ...NONE, reason: "not available" });
      else console.warn("[real-usage] poll threw, keeping last value:", e);
    }
  }, [envId]);

  useEffect(() => {
    mounted.current = true;
    seenLive.current = false;
    setReal(NONE);
    setLoading(true);

    const callEnvId = envId;
    refresh().finally(() => {
      // Only clear loading if we're still on the same env this initial fetch
      // was for; otherwise the new env's effect will manage loading itself.
      if (mounted.current && activeEnvId.current === callEnvId) setLoading(false);
    });

    const id = setInterval(refresh, POLL_MS);
    return () => {
      mounted.current = false;
      clearInterval(id);
    };
  }, [refresh, envId]);

  return { real, loading, refresh };
}

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
export function useRealUsage(envId: string): { real: RealUsage; refresh: () => void } {
  const [real, setReal] = useState<RealUsage>(NONE);
  const mounted = useRef(true);

  const refresh = useCallback(async () => {
    try {
      const res = await invoke<RealUsage>("get_real_usage", { envId });
      if (mounted.current) setReal(res);
    } catch {
      if (mounted.current) setReal({ ...NONE, reason: "not available" });
    }
  }, [envId]);

  useEffect(() => {
    mounted.current = true;
    refresh();
    const id = setInterval(refresh, POLL_MS);
    return () => {
      mounted.current = false;
      clearInterval(id);
    };
  }, [refresh]);

  return { real, refresh };
}

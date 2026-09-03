import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Attribution } from "../lib/types";

const POLL_MS = 60_000;

const NONE: Attribution = {
  found: false,
  generatedAt: 0,
  scannedFiles: 0,
  day: { requestCount: 0, sessionCount: 0, behaviors: [], agents: [], skills: [], plugins: [], mcpServers: [] },
  week: { requestCount: 0, sessionCount: 0, behaviors: [], agents: [], skills: [], plugins: [], mcpServers: [] },
  sessions: [],
};

interface UseAttribution {
  attr: Attribution;
  loading: boolean;
  refresh: () => void;
}

// Loads local usage attribution from the Rust `get_attribution` command — the
// "what's contributing to your limits" data and per-session accounting that the
// live usage API doesn't serve.
//
// Unlike `useUsage` this is gated on `enabled`: the scan reads every transcript
// touched in the last 7 days, so it only runs while a view that needs it is on
// screen. There is no mock fallback — outside Tauri, or with no local logs, the
// views render their own empty state from `found: false`.
//
// Stale-result protection matches useUsage: the env is captured at call time and
// compared on resolution, so an in-flight call for the previous account can't
// overwrite the current one.
export function useAttribution(envId: string, enabled: boolean): UseAttribution {
  const [attr, setAttr] = useState<Attribution>(NONE);
  const [loading, setLoading] = useState(false);
  const mounted = useRef(true);
  const activeEnv = useRef(envId);
  activeEnv.current = envId;

  const refresh = useCallback(async () => {
    const callEnv = envId;
    try {
      const res = await invoke<Attribution>("get_attribution", { envId });
      if (!mounted.current || activeEnv.current !== callEnv) return;
      setAttr(res);
    } catch {
      if (mounted.current && activeEnv.current === callEnv) setAttr(NONE);
    } finally {
      if (mounted.current && activeEnv.current === callEnv) setLoading(false);
    }
  }, [envId]);

  // Drop the previous account's data rather than showing it under a new label.
  // Keyed on the env alone, so toggling tabs keeps what was already scanned.
  const loadedEnv = useRef<string | null>(null);
  useEffect(() => {
    if (loadedEnv.current !== null && loadedEnv.current !== envId) setAttr(NONE);
  }, [envId]);

  useEffect(() => {
    mounted.current = true;
    if (!enabled) {
      return () => {
        mounted.current = false;
      };
    }
    // Only show a spinner the first time an env is scanned; a poll or a return
    // to the tab refreshes in place.
    if (loadedEnv.current !== envId) {
      loadedEnv.current = envId;
      setLoading(true);
    }
    refresh();
    const id = setInterval(refresh, POLL_MS);
    return () => {
      mounted.current = false;
      clearInterval(id);
    };
  }, [refresh, enabled, envId]);

  return { attr, loading, refresh };
}

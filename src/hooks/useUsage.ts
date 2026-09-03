import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { UsageData } from "../lib/types";
import { mockUsage } from "../lib/mockData";

const POLL_MS = 30_000;

interface UseUsage {
  data: UsageData;
  loading: boolean;
  refresh: () => void;
}

// Loads aggregated usage from the Rust `get_usage` command for the given
// environment and polls every 30s. Falls back to bundled mock data when running
// outside Tauri or when no local Claude logs exist (backend returns isMock=true).
//
// `sessionStartMs` is the live session window start (reset − 5h) when known, so
// the backend's per-model split lines up with the ring; it changes only when
// the window rolls over, which re-runs the effect.
//
// Stale-result protection: when the user switches envs, an in-flight call for
// the old env must not overwrite the new env's data. We capture the call key at
// call time and compare against the latest on resolution.
export function useUsage(envId: string, sessionStartMs?: number): UseUsage {
  const [data, setData] = useState<UsageData>(() => mockUsage());
  const [loading, setLoading] = useState(true);
  const mounted = useRef(true);
  const key = `${envId}|${sessionStartMs ?? ""}`;
  const activeKey = useRef(key);
  activeKey.current = key;

  const refresh = useCallback(async () => {
    const callKey = key;
    try {
      const res = await invoke<UsageData>("get_usage", { envId, sessionStartMs: sessionStartMs ?? null });
      if (!mounted.current || activeKey.current !== callKey) return;
      setData(res.isMock ? mockUsage() : res);
    } catch {
      if (mounted.current && activeKey.current === callKey) setData(mockUsage());
    } finally {
      if (mounted.current && activeKey.current === callKey) setLoading(false);
    }
  }, [envId, sessionStartMs, key]);

  useEffect(() => {
    mounted.current = true;
    setLoading(true);
    refresh();
    const id = setInterval(refresh, POLL_MS);
    return () => {
      mounted.current = false;
      clearInterval(id);
    };
  }, [refresh]);

  return { data, loading, refresh };
}

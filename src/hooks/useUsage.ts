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
// Stale-result protection: when the user switches envs, an in-flight call for
// the old env must not overwrite the new env's data. We capture envId at call
// time and compare against `activeEnvId.current` on resolution.
export function useUsage(envId: string): UseUsage {
  const [data, setData] = useState<UsageData>(() => mockUsage());
  const [loading, setLoading] = useState(true);
  const mounted = useRef(true);
  const activeEnvId = useRef(envId);
  activeEnvId.current = envId;

  const refresh = useCallback(async () => {
    const callEnvId = envId;
    try {
      const res = await invoke<UsageData>("get_usage", { envId: callEnvId });
      if (!mounted.current || activeEnvId.current !== callEnvId) return;
      setData(res.isMock ? mockUsage() : res);
    } catch {
      if (mounted.current && activeEnvId.current === callEnvId) setData(mockUsage());
    } finally {
      if (mounted.current && activeEnvId.current === callEnvId) setLoading(false);
    }
  }, [envId]);

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

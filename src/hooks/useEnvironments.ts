import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Account, Environment } from "../lib/types";

const POLL_MS = 60_000;

const EMPTY_ACCOUNT: Account = {
  found: false,
  email: "",
  orgName: "",
  orgType: "",
  seatTier: "",
  userRateLimitTier: "",
  detectedPlan: "",
};

// Discovers Claude environments (~/.claude* config dirs) via the backend and
// resolves any user-added custom paths into the same list. Polls occasionally so
// switching accounts in Claude Code is reflected.
export function useEnvironments(customPaths: string[]): { environments: Environment[]; refresh: () => void } {
  const [environments, setEnvironments] = useState<Environment[]>([]);
  const mounted = useRef(true);

  const refresh = useCallback(async () => {
    let discovered: Environment[] = [];
    try {
      discovered = await invoke<Environment[]>("list_environments");
    } catch {
      discovered = [];
    }
    const knownIds = new Set(discovered.map((e) => e.id));

    // Resolve custom paths not already discovered.
    const customs: Environment[] = [];
    for (const path of customPaths) {
      if (!path || knownIds.has(path)) continue;
      let account = EMPTY_ACCOUNT;
      try {
        account = await invoke<Account>("get_account", { envId: path });
      } catch {
        /* keep empty */
      }
      customs.push({
        id: path,
        label: account.orgName || account.email || path,
        account,
        hasLogs: true,
        custom: true,
      });
    }

    if (mounted.current) setEnvironments([...discovered, ...customs]);
  }, [customPaths]);

  useEffect(() => {
    mounted.current = true;
    refresh();
    const id = setInterval(refresh, POLL_MS);
    return () => {
      mounted.current = false;
      clearInterval(id);
    };
  }, [refresh]);

  return { environments, refresh };
}

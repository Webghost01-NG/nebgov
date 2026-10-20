"use client";

import { useCallback, useEffect, useState } from "react";
import { GuardianCouncilClient, type PendingAction } from "@nebgov/sdk";
import { readGuardianCouncilConfig } from "../lib/nebgov-env";

/** Upper bound on actions read per load; the contract has no counter view. */
const MAX_ACTIONS = 100;

export function buildGuardianCouncilClient(
  simulationAccount?: string | null,
): GuardianCouncilClient | null {
  const config = readGuardianCouncilConfig(simulationAccount);
  return config ? new GuardianCouncilClient(config) : null;
}

export interface GuardianCouncilState {
  members: string[];
  threshold: number;
  governor: string | null;
  /** Action expiry window in ledgers; 0 means actions never expire. */
  actionTtl: number;
  /** Every action in id order, including executed ones. */
  actions: PendingAction[];
}

interface UseGuardianCouncilResult {
  data: GuardianCouncilState | null;
  /** True when no council address is set in the environment. */
  notConfigured: boolean;
  /** True when configured but there is no account to simulate reads from. */
  needsAccount: boolean;
  loading: boolean;
  error: string | null;
  refetch: () => void;
}

/**
 * Load the council's members, threshold, governor, action TTL and actions.
 * Views are simulated from `simulationAccount` (normally the connected wallet),
 * so nothing loads until one is available.
 */
export function useGuardianCouncil(
  simulationAccount: string | null,
): UseGuardianCouncilResult {
  const [data, setData] = useState<GuardianCouncilState | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [refetchToken, setRefetchToken] = useState(0);

  const config = readGuardianCouncilConfig(simulationAccount);
  const notConfigured = config === null;
  const needsAccount = config !== null && !config.simulationAccount;
  const canRead = config !== null && !needsAccount;

  useEffect(() => {
    if (!canRead) {
      setLoading(false);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setError(null);

    const council = buildGuardianCouncilClient(simulationAccount);
    if (!council) return;
    Promise.all([
      council.getMembers(),
      council.getThreshold(),
      council.getGovernor(),
      council.getActionTtl(),
      council.listActions(0n, MAX_ACTIONS),
    ])
      .then(([members, threshold, governor, actionTtl, actions]) => {
        if (!cancelled) setData({ members, threshold, governor, actionTtl, actions });
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [canRead, simulationAccount, refetchToken]);

  const refetch = useCallback(() => setRefetchToken((n) => n + 1), []);

  return { data, notConfigured, needsAccount, loading, error, refetch };
}

/** Plain-English summary of a council action, e.g. "Cancel active proposal #4". */
export function describeGuardianAction(action: PendingAction["action"]): string {
  switch (action.tag) {
    case "Pause":
      return "Pause the governor";
    case "Unpause":
      return "Unpause the governor";
    case "CancelActive":
      return `Cancel active proposal #${action.proposalId}`;
    case "CancelQueued":
      return `Cancel queued proposal #${action.proposalId}`;
    case "RotateMember":
      return `Rotate member ${action.old} → ${action.new}`;
    case "SetThreshold":
      return `Set approval threshold to ${action.threshold}`;
    case "SetActionTtl":
      return action.ttlLedgers === 0
        ? "Disable action expiry"
        : `Set action expiry to ${action.ttlLedgers} ledgers`;
  }
}

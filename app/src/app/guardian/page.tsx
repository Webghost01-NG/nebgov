"use client";

/**
 * Guardian council page.
 *
 * Shows the M-of-N emergency council that can pause or cancel proposals on the
 * governor: its members and threshold, plus every action with its approval
 * progress. Council members can approve or revoke their approval, and anyone
 * with a connected wallet can execute an action once it has enough approvals
 * (the contract's `execute_action` is permissionless). See
 * contracts/guardian-council and GuardianCouncilClient in the SDK.
 */

import { useMemo, useState } from "react";
import toast from "react-hot-toast";
import type { PendingAction } from "@nebgov/sdk";
import { useWallet } from "../../lib/wallet-context";
import {
  buildGuardianCouncilClient,
  describeGuardianAction,
  useGuardianCouncil,
} from "../../hooks/useGuardianCouncil";
import { useLedgerClock } from "../../lib/hooks/useLedgerClock";
import { AddressDisplay } from "../../components/AddressDisplay";
import { ErrorState } from "../../components/ErrorState";
import { Skeleton } from "../../components/ui/Skeleton";

type ActionKind = "approve" | "revoke" | "execute";

const ACTION_LABELS: Record<ActionKind, { idle: string; busy: string; done: string }> = {
  approve: { idle: "Approve", busy: "Approving...", done: "Approval submitted." },
  revoke: { idle: "Revoke approval", busy: "Revoking...", done: "Approval revoked." },
  execute: { idle: "Execute", busy: "Executing...", done: "Action executed." },
};

function CouncilSkeleton() {
  return (
    <div className="space-y-4" aria-label="Loading guardian council" role="status">
      <div className="border border-gray-200 dark:border-gray-700 rounded-xl p-4">
        <Skeleton className="h-4 w-1/3 mb-3" />
        <Skeleton className="h-3 w-2/3 mb-2" />
        <Skeleton className="h-3 w-1/2" />
      </div>
      {[0, 1].map((i) => (
        <div key={i} className="border border-gray-200 dark:border-gray-700 rounded-xl p-4">
          <Skeleton className="h-4 w-1/2 mb-3" />
          <Skeleton className="h-2 w-full" />
        </div>
      ))}
    </div>
  );
}

interface ActionCardProps {
  action: PendingAction;
  members: string[];
  threshold: number;
  actionTtl: number;
  currentLedger: number;
  publicKey: string | null;
  busy: string | null;
  onAct: (kind: ActionKind, action: PendingAction) => void;
}

function ActionCard({
  action,
  members,
  threshold,
  actionTtl,
  currentLedger,
  publicKey,
  busy,
  onAct,
}: ActionCardProps) {
  // Only approvals from sitting members count at execution time, so a rotated-out
  // member's approval is shown but not counted.
  const liveApprovals = action.approvals.filter((a) => members.includes(a));
  const staleApprovals = action.approvals.length - liveApprovals.length;
  const thresholdMet = liveApprovals.length >= threshold;
  const expiresAt = actionTtl > 0 ? action.createdLedger + actionTtl : null;
  const expired = expiresAt !== null && currentLedger > 0 && currentLedger > expiresAt;

  const isMember = publicKey !== null && members.includes(publicKey);
  const hasApproved = publicKey !== null && action.approvals.includes(publicKey);
  const pct = threshold > 0 ? Math.min(100, (liveApprovals.length / threshold) * 100) : 0;
  const key = (kind: ActionKind) => `${action.id}:${kind}`;

  function button(kind: ActionKind, className: string) {
    const label = ACTION_LABELS[kind];
    const working = busy === key(kind);
    return (
      <button
        type="button"
        onClick={() => onAct(kind, action)}
        disabled={busy !== null}
        className={`px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50 ${className}`}
      >
        {working ? label.busy : label.idle}
      </button>
    );
  }

  return (
    <li className="border border-gray-200 dark:border-gray-700 rounded-xl p-4" data-testid={`action-${action.id}`}>
      <div className="flex items-start justify-between gap-3 mb-1">
        <h3 className="font-medium text-gray-900 dark:text-gray-100 break-all">
          #{action.id.toString()} · {describeGuardianAction(action.action)}
        </h3>
        <span
          className={`shrink-0 rounded-full px-2.5 py-0.5 text-xs font-medium ${
            action.executed
              ? "bg-gray-100 text-gray-700 dark:bg-gray-800 dark:text-gray-300"
              : expired
                ? "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-300"
                : thresholdMet
                  ? "bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-300"
                  : "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-300"
          }`}
        >
          {action.executed ? "Executed" : expired ? "Expired" : thresholdMet ? "Ready to execute" : "Pending"}
        </span>
      </div>

      <div className="text-xs text-gray-500 dark:text-gray-400 mb-3">
        Proposed by <AddressDisplay address={action.proposer} /> at ledger {action.createdLedger}
        {expiresAt !== null && !action.executed ? ` · expires after ledger ${expiresAt}` : ""}
      </div>

      <div className="mb-1 flex items-center justify-between text-xs text-gray-600 dark:text-gray-300">
        <span>
          {liveApprovals.length} of {threshold} approvals
        </span>
        {staleApprovals > 0 && (
          <span className="text-gray-400">
            +{staleApprovals} from former member{staleApprovals === 1 ? "" : "s"} (not counted)
          </span>
        )}
      </div>
      <div
        className="h-2 w-full rounded-full bg-gray-200 dark:bg-gray-700 overflow-hidden"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={threshold}
        aria-valuenow={liveApprovals.length}
        aria-label={`Approvals for action ${action.id.toString()}`}
      >
        <div className={`h-full ${thresholdMet ? "bg-green-500" : "bg-indigo-500"}`} style={{ width: `${pct}%` }} />
      </div>

      {action.approvals.length > 0 && (
        <ul className="mt-3 flex flex-wrap gap-2 text-xs text-gray-600 dark:text-gray-300">
          {action.approvals.map((approver) => (
            <li key={approver} className="rounded bg-gray-100 dark:bg-gray-800 px-2 py-1">
              <AddressDisplay address={approver} />
              {approver === publicKey ? " (you)" : ""}
            </li>
          ))}
        </ul>
      )}

      {!action.executed && publicKey && (
        <div className="mt-4 flex flex-wrap gap-2">
          {isMember && !hasApproved && !expired && button("approve", "bg-indigo-600 text-white hover:bg-indigo-700")}
          {isMember && hasApproved && button("revoke", "border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-200 hover:bg-gray-50 dark:hover:bg-gray-800")}
          {thresholdMet && !expired && button("execute", "bg-green-600 text-white hover:bg-green-700")}
        </div>
      )}
      {!action.executed && !publicKey && (
        <p className="mt-3 text-xs text-gray-400 dark:text-gray-500">Connect a wallet to approve or execute.</p>
      )}
    </li>
  );
}

export default function GuardianPage() {
  const { isConnected, publicKey, signTransaction } = useWallet();
  const { data, notConfigured, needsAccount, loading, error, refetch } = useGuardianCouncil(publicKey);
  const { currentLedger } = useLedgerClock();
  const [busy, setBusy] = useState<string | null>(null);
  const [showExecuted, setShowExecuted] = useState(false);

  const visibleActions = useMemo(() => {
    const actions = data?.actions ?? [];
    return actions.filter((a) => showExecuted || !a.executed).slice().reverse();
  }, [data, showExecuted]);
  const executedCount = data ? data.actions.filter((a) => a.executed).length : 0;

  async function handleAct(kind: ActionKind, action: PendingAction) {
    if (!publicKey || !signTransaction) return;
    const client = buildGuardianCouncilClient(publicKey);
    if (!client) return;

    setBusy(`${action.id}:${kind}`);
    try {
      if (kind === "approve") await client.approveActionWithSign(publicKey, action.id, signTransaction);
      else if (kind === "revoke") await client.revokeApprovalWithSign(publicKey, action.id, signTransaction);
      else await client.executeActionWithSign(publicKey, action.id, signTransaction);
      toast.success(ACTION_LABELS[kind].done);
      refetch();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : `Could not ${kind} the action`);
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="max-w-4xl mx-auto px-4 py-8">
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-gray-900 dark:text-gray-100">Guardian Council</h1>
        <p className="text-sm text-gray-500 dark:text-gray-300">
          The council&apos;s members can pause the governor or cancel proposals in an emergency. An action
          runs once enough members approve it.
        </p>
      </div>

      {notConfigured && (
        <ErrorState
          title="Guardian council not configured"
          message="Set NEXT_PUBLIC_GUARDIAN_COUNCIL_ADDRESS to the deployed council contract."
        />
      )}

      {!notConfigured && needsAccount && (
        <div className="text-center py-16 text-gray-500 dark:text-gray-400">
          {isConnected ? "Loading wallet..." : "Connect your wallet to view the guardian council."}
        </div>
      )}

      {!notConfigured && !needsAccount && loading && !data && <CouncilSkeleton />}

      {!notConfigured && !needsAccount && !loading && error && (
        <ErrorState title="Could not load the guardian council" message={error} onRetry={refetch} />
      )}

      {!notConfigured && !needsAccount && !error && data && (
        <div className="space-y-6">
          <section
            className="border border-gray-200 dark:border-gray-700 rounded-xl p-4"
            aria-label="Council members"
          >
            <div className="flex items-baseline justify-between mb-3">
              <h2 className="font-semibold text-gray-900 dark:text-gray-100">Members</h2>
              <span className="text-sm text-gray-600 dark:text-gray-300">
                {data.threshold} of {data.members.length} must approve
              </span>
            </div>
            <ul className="flex flex-wrap gap-2 text-sm">
              {data.members.map((member) => (
                <li key={member} className="rounded bg-gray-100 dark:bg-gray-800 px-2 py-1 text-gray-700 dark:text-gray-200">
                  <AddressDisplay address={member} />
                  {member === publicKey ? " (you)" : ""}
                </li>
              ))}
            </ul>
            <dl className="mt-3 grid gap-1 text-xs text-gray-500 dark:text-gray-400">
              {data.governor && (
                <div className="flex gap-2">
                  <dt>Governor:</dt>
                  <dd>
                    <AddressDisplay address={data.governor} />
                  </dd>
                </div>
              )}
              <div className="flex gap-2">
                <dt>Action expiry:</dt>
                <dd>{data.actionTtl === 0 ? "never" : `${data.actionTtl} ledgers after proposal`}</dd>
              </div>
            </dl>
          </section>

          <section aria-label="Council actions">
            <div className="flex items-center justify-between mb-3">
              <h2 className="font-semibold text-gray-900 dark:text-gray-100">
                {showExecuted ? "All actions" : "Pending actions"}
              </h2>
              {executedCount > 0 && (
                <label className="flex items-center gap-2 text-sm text-gray-600 dark:text-gray-300">
                  <input
                    type="checkbox"
                    checked={showExecuted}
                    onChange={(e) => setShowExecuted(e.target.checked)}
                  />
                  Show executed ({executedCount})
                </label>
              )}
            </div>

            {visibleActions.length === 0 ? (
              <div className="text-center py-12 text-gray-500 dark:text-gray-400">
                No pending council actions.
              </div>
            ) : (
              <ul className="space-y-4">
                {visibleActions.map((action) => (
                  <ActionCard
                    key={action.id.toString()}
                    action={action}
                    members={data.members}
                    threshold={data.threshold}
                    actionTtl={data.actionTtl}
                    currentLedger={currentLedger}
                    publicKey={publicKey}
                    busy={busy}
                    onAct={handleAct}
                  />
                ))}
              </ul>
            )}
          </section>
        </div>
      )}
    </div>
  );
}

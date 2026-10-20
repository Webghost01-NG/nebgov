import type { GovernorConfig, GuardianCouncilConfig, Network } from "@nebgov/sdk";
import { backendBaseUrl } from "./backend";

/** Governor client config from Next public env (or null if misconfigured). */
export function readGovernorConfig(): GovernorConfig | null {
  const governorAddress = process.env.NEXT_PUBLIC_GOVERNOR_ADDRESS;
  const timelockAddress = process.env.NEXT_PUBLIC_TIMELOCK_ADDRESS;
  const votesAddress = process.env.NEXT_PUBLIC_VOTES_ADDRESS;
  const coSponsorshipAddress = process.env.NEXT_PUBLIC_CO_SPONSORSHIP_ADDRESS;
  const convictionVotingAddress = process.env.NEXT_PUBLIC_CONVICTION_VOTING_ADDRESS;
  const signalAnchorAddress = process.env.NEXT_PUBLIC_SIGNAL_ANCHOR_ADDRESS;
  const proposalBondsAddress = process.env.NEXT_PUBLIC_PROPOSAL_BONDS_ADDRESS;
  const treasuryStrategiesAddress = process.env.NEXT_PUBLIC_TREASURY_STRATEGIES_ADDRESS;
  const optimisticGovernorAddress = process.env.NEXT_PUBLIC_OPTIMISTIC_GOVERNOR_ADDRESS;
  const votingRewardsAddress = process.env.NEXT_PUBLIC_VOTING_REWARDS_ADDRESS;
  const network = (process.env.NEXT_PUBLIC_NETWORK || "testnet") as Network;
  const rpcUrl = process.env.NEXT_PUBLIC_RPC_URL;

  if (!governorAddress || !timelockAddress || !votesAddress) return null;

  return {
    governorAddress,
    timelockAddress,
    votesAddress,
    network,
    backendUrl: backendBaseUrl(),
    ...(rpcUrl ? { rpcUrl } : {}),
    ...(coSponsorshipAddress ? { coSponsorshipAddress } : {}),
    ...(convictionVotingAddress ? { convictionVotingAddress } : {}),
    ...(signalAnchorAddress ? { signalAnchorAddress } : {}),
    ...(proposalBondsAddress ? { proposalBondsAddress } : {}),
    ...(treasuryStrategiesAddress ? { treasuryStrategiesAddress } : {}),
    ...(optimisticGovernorAddress ? { optimisticGovernorAddress } : {}),
    ...(votingRewardsAddress ? { votingRewardsAddress } : {}),
  };
}

export function subscriptionOptsFromConfig(config: GovernorConfig): {
  network: Network;
  rpcUrl?: string;
} {
  return { network: config.network, ...(config.rpcUrl ? { rpcUrl: config.rpcUrl } : {}) };
}

/** Returns the indexer base URL or null if not configured. */
export function readIndexerUrl(): string | null {
  return process.env.NEXT_PUBLIC_INDEXER_URL ?? null;
}

/**
 * Guardian council client config from Next public env, or null when the
 * council address is not configured. Reads need a funded account to simulate
 * from, so the caller supplies the connected wallet; otherwise the optional
 * `NEXT_PUBLIC_SIMULATION_ACCOUNT` is used.
 */
export function readGuardianCouncilConfig(
  simulationAccount?: string | null,
): GuardianCouncilConfig | null {
  const guardianCouncilAddress = process.env.NEXT_PUBLIC_GUARDIAN_COUNCIL_ADDRESS;
  if (!guardianCouncilAddress) return null;
  const rpcUrl = process.env.NEXT_PUBLIC_RPC_URL;
  const account = simulationAccount || process.env.NEXT_PUBLIC_SIMULATION_ACCOUNT;
  return {
    guardianCouncilAddress,
    network: (process.env.NEXT_PUBLIC_NETWORK || "testnet") as Network,
    ...(rpcUrl ? { rpcUrl } : {}),
    ...(account ? { simulationAccount: account } : {}),
  };
}

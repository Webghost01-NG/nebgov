/**
 * NebGov SDK — core types
 */

import type { RetryOptions } from "../utils";

/** Stellar network identifier. */
export type Network = "mainnet" | "testnet" | "futurenet";

/** On-chain lifecycle state of a governance proposal. */
export enum ProposalState {
  /** Created but voting has not started yet. */
  Pending = "Pending",
  /** Voting is currently open. */
  Active = "Active",
  /** Voting closed; quorum or majority not reached. */
  Defeated = "Defeated",
  /** Voting closed; quorum and majority reached — awaiting queue or execution. */
  Succeeded = "Succeeded",
  /** Queued in the timelock, awaiting the execution delay. */
  Queued = "Queued",
  /** Successfully executed on-chain. */
  Executed = "Executed",
  /** Cancelled by the proposer or guardian. */
  Cancelled = "Cancelled",
  /** Queued but not executed before the execution deadline. */
  Expired = "Expired",
}

/** Thrown when an unrecognised on-chain proposal-state variant is encountered. */
export class UnknownProposalStateError extends Error {
  constructor(variant: string) {
    super(`Unknown proposal state: ${variant}`);
    this.name = "UnknownProposalStateError";
  }
}

/** How a voter casts their ballot. */
export enum VoteSupport {
  /** Vote against the proposal. */
  Against = 0,
  /** Vote in favour of the proposal. */
  For = 1,
  /** Formally participate without choosing a side. */
  Abstain = 2,
}

/** Voting mechanism used by the governor. */
export enum VoteType {
  /** One token = one vote. */
  Simple = "Simple",
  /** Extended voting with configurable time-weight. */
  Extended = "Extended",
  /** Square-root of token balance used as vote weight. */
  Quadratic = "Quadratic",
}

/** Full on-chain representation of a governance proposal. */
export interface Proposal {
  /** Unique numeric identifier assigned at creation. */
  id: bigint;
  /** Stellar address that submitted the proposal. */
  proposer: string;
  /** Human-readable summary stored on-chain. */
  description: string;
  /** Ledger sequence at which voting opens. */
  startLedger: number;
  /** Ledger sequence at which voting closes. */
  endLedger: number;
  /** Accumulated votes in favour. */
  votesFor: bigint;
  /** Accumulated votes against. */
  votesAgainst: bigint;
  /** Accumulated abstain votes. */
  votesAbstain: bigint;
  /** Whether the proposal has been executed. */
  executed: boolean;
  /** Whether the proposal was cancelled. */
  cancelled: boolean;
}

/** Input parameters for creating a single-action proposal. */
export interface ProposalInput {
  /** Human-readable proposal summary. */
  description: string;
  /** Target contract address for the on-chain action. */
  target: string;
  /** Function name to invoke on the target. */
  fnName: string;
  /** ABI-encoded call arguments. */
  calldata: Buffer | Uint8Array;
}

/**
 * A co-sponsorship pre-proposal, awaiting enough pledged voting power to
 * meet the governor's proposal threshold before being promoted into a real
 * governor proposal via `CoSponsorshipClient.finalizeDraft`.
 */
export interface ProposalDraft {
  /** Unique numeric identifier assigned at creation. */
  id: bigint;
  /** Stellar address that created the draft. */
  creator: string;
  /** Human-readable summary. */
  description: string;
  /** SHA-256 hash of the off-chain description content. */
  descriptionHash: string;
  /** URI pointing to the full proposal description content. */
  metadataUri: string;
  /** Contract addresses that will be invoked if the draft is finalized. */
  targets: string[];
  /** Function names invoked on each target. */
  fnNames: string[];
  /** ABI-encoded calldata for each target. */
  calldatas: (Buffer | Uint8Array)[];
  /** Ledger sequence at which the draft was created. */
  createdLedger: number;
  /** Ledger sequence after which the draft can no longer be co-sponsored or finalized. */
  expiryLedger: number;
  /** Addresses that have pledged voting power to this draft. */
  coSponsors: string[];
  /** Voting power pledged by each address in `coSponsors`, same order. */
  coSponsorPower: bigint[];
  /** Sum of all pledged co-sponsor voting power. */
  totalPower: bigint;
  /** Whether the draft has been promoted into a real proposal. */
  finalized: boolean;
  /** Whether the draft was cancelled. */
  cancelled: boolean;
}

/** A gasless off-chain signaling poll ("temperature check") — see SignalingClient. */
export interface SignalingPoll {
  id: number;
  creatorAddress: string;
  title: string;
  description: string;
  choices: string[];
  /** Ledger at which voting power is snapshot-checkpointed for weighting votes. */
  snapshotLedger: number;
  startTime: string;
  endTime: string;
  finalized: boolean;
  /** SHA-256 hex digest of the finalized tally, set once the poll closes. */
  resultHash: string | null;
  /** Transaction hash of the on-chain anchor submission, if anchoring was enabled. */
  anchoredTxHash: string | null;
  createdAt: string;
}

/** Weighted tally for a signaling poll — live before finalization, final after. */
export interface SignalingPollResults {
  finalized: boolean;
  choices: string[];
  /** Weighted voting power per choice, aligned by index with `choices`. */
  totals: bigint[];
  totalVotes: number;
  totalWeight: bigint;
  resultHash?: string | null;
  anchoredTxHash?: string | null;
}

/** An immutable on-chain record anchoring a finalized signaling poll's result hash. */
export interface SignalAnchorRecord {
  pollId: bigint;
  resultHash: string;
  anchoredLedger: number;
  anchorer: string;
}

/** Lifecycle state of a proposer bond — see `ProposalBondsClient`. */
export type BondState = "Locked" | "Refunded" | "Slashed";

/**
 * A refundable bond posted by a proposer alongside a governor proposal
 * (correlated by shared `descriptionHash`), refunded once the proposal
 * reaches a terminal state or slashed by a follow-up governance vote if
 * judged spam, duplicated, or malicious.
 */
export interface ProposalBond {
  /** Stellar address that posted the bond. */
  proposer: string;
  /** SHA-256 hash of the off-chain description content, shared with the correlated proposal. */
  descriptionHash: string;
  /** Bonded amount, in the configured bond token's smallest unit. */
  amount: bigint;
  /** Ledger sequence at which the bond was locked. */
  lockedLedger: number;
  /** Current lifecycle state. */
  state: BondState;
}

/** Read-only configuration snapshot of a deployed ProposalBonds contract. */
export interface ProposalBondSettings {
  /** Token address bonds are denominated and paid in. */
  bondToken: string;
  /** Amount, in the bond token's smallest unit, `lock_bond` currently requires. */
  bondAmount: bigint;
  /** Governor contract address this bonds registry is attached to. */
  governor: string;
  /**
   * Ledgers after a correlated proposal's `end_ledger` during which
   * `refund_bond` is blocked, giving the community time to submit a
   * `slash` governance proposal first.
   */
  refundGraceLedgers: number;
  /**
   * Ledgers after `lock_bond` after which `refund_bond` succeeds
   * unconditionally — the escape hatch for a proposal stuck in `Queued`
   * with no governor-reported terminal state (see the contract's
   * `refund_bond` doc comment).
   */
  maxLockLedgers: number;
}

/** Aggregated vote tallies for a proposal. */
export interface ProposalVotes {
  /** Total tokens cast in favour. */
  votesFor: bigint;
  /** Total tokens cast against. */
  votesAgainst: bigint;
  /** Total tokens cast as abstain. */
  votesAbstain: bigint;
}

/**
 * Inputs to a two-phase commit-reveal vote (Issue #766).
 *
 * `weightSeed` and `salt` hide the vote's existence during the commit phase
 * and are only disclosed at reveal time; if omitted from
 * {@link GovernorClient.generateCommitment}, cryptographically random values
 * are generated for you and returned in the {@link CommitmentResult} so they
 * can be persisted (e.g. to `localStorage`) until reveal.
 */
export interface CommitVoteParams {
  proposalId: bigint;
  support: VoteSupport;
  weightSeed?: bigint;
  salt?: Buffer;
}

/** Output of {@link GovernorClient.generateCommitment}. */
export interface CommitmentResult {
  /** `sha256(proposal_id_le || support_u8 || weight_seed_le || salt)`, exactly matching the on-chain preimage. */
  commitment: Buffer;
  /** The salt used — persist this for the later `reveal_vote` call. */
  salt: Buffer;
  /** The weight seed used — persist this for the later `reveal_vote` call. */
  weightSeed: bigint;
}

/** Where a proposal's commit-reveal voting currently stands. */
export interface CommitRevealStatus {
  phase: "commit" | "reveal" | "ended";
  commitDeadline: number;
  revealDeadline: number;
  /**
   * Best-effort counts. `get_commit_count`/`get_reveal_count` are not
   * on-chain read functions — see the scope-boundary note in
   * `contracts/governor/src/commit_reveal.rs` — so these are sourced from
   * the indexer's mirror of the `VoteCommitted`/`VoteRevealed` event stream
   * (same pattern as `ReputationClient`'s proposer leaderboard) and are `0`
   * if no `indexerUrl` is configured on the client.
   */
  commitCount: number;
  revealCount: number;
}


export interface GovernorConfig {
  /** Contract address of the governor */
  governorAddress: string;
  /** Contract address of the timelock */
  timelockAddress: string;
  /** Contract address of the token-votes contract */
  votesAddress: string;
  /** Contract address of the co-sponsorship registry, if deployed */
  coSponsorshipAddress?: string;
  /** Contract address of the vote-escrow contract, if deployed */
  voteEscrowAddress?: string;
  /** Contract address of the independent conviction-voting module, if deployed. */
  convictionVotingAddress?: string;
  /** Contract address of the gasless-signaling result anchor, if deployed */
  signalAnchorAddress?: string;
  /** Contract address of the proposal-bonds registry, if deployed */
  proposalBondsAddress?: string;
  /** Contract address of the treasury yield-strategy allocator, if deployed */
  treasuryStrategiesAddress?: string;
  /** Contract address of the optimistic-governance track, if deployed */
  optimisticGovernorAddress?: string;
  /** Contract address of the voting-participation rewards program, if deployed */
  votingRewardsAddress?: string;
  /** Stellar network to connect to */
  network: Network;
  /** RPC URL override (optional — defaults to public horizon) */
  rpcUrl?: string;
  /** Optional funded classic account used for read-only simulation calls. */
  simulationAccount?: string;
  /** Indexer base URL for off-chain queries (e.g. getVotingHistory) */
  indexerUrl?: string;
  /** Backend base URL for off-chain queries/mutations (e.g. SignalingClient's poll CRUD) */
  backendUrl?: string;
  /** Maximum number of retry attempts for RPC calls (default: 3) */
  maxAttempts?: number;
  /** Base delay in milliseconds for exponential backoff (default: 1000) */
  baseDelayMs?: number;
  /** Retry policy for all operations made by this client. */
  retry?: RetryOptions;
  /** Token decimals for vote display (optional — fetched from contract if not provided) */
  decimals?: number;
  /** Admin secret for admin-only backend endpoints (e.g. governance tuning config update) */
  adminSecret?: string;
}

export interface ConvictionProposal {
  id: bigint;
  proposer: string;
  target: string;
  fnName: string;
  calldata: Buffer | Uint8Array;
  requestedAmount: bigint;
  createdLedger: number;
  conviction: bigint;
  lastUpdatedLedger: number;
  executed: boolean;
  cancelled: boolean;
}

export interface ConvictionSnapshot {
  proposalId: bigint;
  ledger: number;
  conviction: bigint;
}

/** Lifecycle state of an optimistic-governance proposal (Issue #993). */
export type OptimisticProposalState =
  | "ChallengeWindow"
  | "Objected"
  | "Passed"
  | "Executed"
  | "Cancelled";

export interface OptimisticProposal {
  id: bigint;
  proposer: string;
  target: string;
  fnName: string;
  calldata: Buffer | Uint8Array;
  descriptionHash: Buffer | Uint8Array;
  createdLedger: number;
  challengeEndLedger: number;
  objectionVotes: bigint;
  state: OptimisticProposalState;
}

export interface OptimisticObjection {
  proposalId: bigint;
  objector: string;
  weight: bigint;
  runningTotal: bigint;
  ledger: number;
}

export interface OptimisticGovernorSettings {
  votesToken: string;
  challengeWindowLedgers: number;
  objectionThresholdBps: number;
  proposerBondAmount: bigint;
  proposerBondToken: string;
}

/** On-chain configuration of a single registered yield strategy (Issue #997). */
export interface Strategy {
  adapter: string;
  token: string;
  maxAllocationBps: number;
  withdrawalCooldownLedgers: number;
  active: boolean;
}

/** A strategy's current on-chain allocation. */
export interface Allocation {
  strategyId: number;
  amount: bigint;
  depositedLedger: number;
}

/** A single point in a strategy's indexer-backed principal-deposited history. */
export interface StrategyPerformancePoint {
  amount: bigint;
  ledger: number;
  createdAt: string;
}

/**
 * A strategy row as tracked by the indexer — includes the `strategyId` and
 * running `currentAllocation` that the on-chain {@link Strategy} read alone
 * doesn't carry (querying every id individually isn't practical for a list
 * view).
 */
export interface IndexedStrategy {
  strategyId: number;
  adapter: string;
  token: string;
  active: boolean;
  currentAllocation: bigint;
  registeredLedger: number;
}

export interface TimelockOperation {
  id: string; // hex-encoded operation hash
  target: string;
  readyAt: bigint;
  expiresAt: bigint;
  executed: boolean;
  cancelled: boolean;
}

export interface TimelockInfo {
  queueLedger: number;
  vetoWindowEndLedger: number;
  executableAtLedger: number;
  executionDeadlineLedger: number;
}

export interface TreasuryTx {
  id: bigint;
  proposer: string;
  target: string;
  approvals: number;
  executed: boolean;
  cancelled: boolean;
}

export interface ProposalAction {
  target: string;
  function: string;
  args: any[];
}

export interface ProposalSimulationResult {
  success: boolean;
  computeUnits?: number;
  stateChanges?: any[];
  error?: string;
}

export interface GovernorEntry {
  id: bigint;
  governor: string;
  timelock: string;
  token: string;
  deployer: string;
}

export interface DeploySettings {
  votingDelay: number;
  votingPeriod: number;
  quorumNumerator: number;
  proposalThreshold: bigint;
  timelockDelay: bigint;
  guardian: string;
  voteType: VoteType;
  proposalGracePeriod: number;
}

export interface FactoryConfig {
  factoryAddress: string;
  network: Network;
  rpcUrl?: string;
  /** Maximum number of retry attempts for RPC calls (default: 3) */
  maxAttempts?: number;
  /** Base delay in milliseconds for exponential backoff (default: 1000) */
  baseDelayMs?: number;
  /** Retry policy for all operations made by this client. */
  retry?: RetryOptions;
}

export interface GuardianActivityEntry {
  proposalId: bigint;
  canceller: string;
  ledger: number;
}

export interface DependencyEdge {
  from: string;
  to: string;
}

export interface DependencyGraph {
  nodes: string[];
  edges: DependencyEdge[];
}

export interface FailedOperation {
  opId: string;
  target: string;
  fnName: string;
  data: string;
  failureReason: string;
  failedAtLedger: number;
  retryCount: number;
}

export interface PartialBatchExecutionState {
  batchOpId: string;
  totalOps: number;
  completedOps: string[];
  failedOps: FailedOperation[];
  pendingOps: string[];
  recoveryMode: boolean;
  recoveryDeadline: number;
}

export interface GovernorSettings {
  votingDelay: number;
  votingPeriod: number;
  quorumNumerator: number;
  proposalThreshold: bigint;
  guardian: string;
  voteType: VoteType;
  proposalGracePeriod: number;
  useDynamicQuorum?: boolean;
  reflectorOracle?: string | null;
  minQuorumUsd?: bigint;
  maxCalldataSize?: number;
  proposalCooldown?: number;
  maxProposalsPerPeriod?: number;
  proposalPeriodDuration?: number;
  /** Whether two-phase commit-reveal voting (Issue #766) applies to new proposals. */
  useCommitReveal?: boolean;
  /** BPS share of a new proposal's voting_period allotted to the commit phase (e.g. 5000 = 50%). */
  commitPhaseFraction?: number;
}

export interface GovernorSettingsValidationLimits {
  maxVotingDelay?: number;
  minVotingPeriod?: number;
}

export interface ExecutionGasEstimate {
  proposalId: bigint;
  actionCount: number;
  calldataBytes: number;
  estimatedCpuInsns: bigint;
  estimatedMemBytes: bigint;
  estimatedFeeStroops: bigint;
  rpcCpuInsns?: bigint;
  rpcMemBytes?: bigint;
}

export interface DelegateInfo {
  address: string;
  votes: bigint;
  percentOfSupply: number;
}

export interface VotesSettings {
  checkpointRetentionPeriod: number;
  timeWeightEnabled: boolean;
  timeWeightScale: number;
}

export interface DelegatorRecord {
  balance: bigint;
  startLedger: number;
}

/**
 * An off-chain-signed instruction to delegate voting power (issue #772).
 * Mirrors `DelegationPermit` in contracts/token-votes/src/delegation_sig.rs.
 */
export interface DelegationPermit {
  delegator: string;
  delegatee: string;
  nonce: bigint;
  expiryLedger: number;
  /** 32 raw bytes — the network ID (sha256 of the network passphrase). */
  chainId: Buffer;
  contractId: string;
}

export interface VoteGasEstimate {
  ok: boolean;
  cpuInsns?: string;
  memBytes?: string;
  estimatedFeeStroops?: string;
  error?: string;
}

// ─── Votes Analytics Types ────────────────────────────────────────────────────

/** A delegate's summary as returned by {@link VotesClient.getTopDelegates}. */
export interface TopDelegate {
  /** Stellar strkey address of the delegate */
  address: string;
  /** Current voting power held by this delegate (effective) */
  votingPower: bigint;
  /** Base token votes (ignoring time-weight) */
  baseVotes: bigint;
  /** Number of accounts currently delegating to this address */
  delegatorCount: number;
}

/** Delegation health statistics as returned by {@link VotesClient.getVotingPowerDistribution}. */
export interface VotingPowerDistribution {
  /** Total voting power currently delegated across all accounts */
  totalDelegated: bigint;
  /** Total token supply from the votes contract */
  totalSupply: bigint;
  /**
   * Fraction of total supply that is actively delegated, expressed as a
   * value between 0 and 1 (e.g. 0.42 means 42% of tokens are delegated).
   */
  delegationRate: number;
  /**
   * Gini coefficient of voting power concentration (0 = perfectly equal,
   * 1 = fully concentrated in one account).
   */
  giniCoefficient: number;
}

/** A single delegator's record as returned by {@link VotesClient.getDelegators}. */
export interface DelegatorInfo {
  /** Stellar strkey address of the delegator */
  delegator: string;
  /** Voting power this delegator contributes to the delegate */
  power: bigint;
}

// ─── Delegation Registry Types (issue #769) ──────────────────────────────────
//
// These mirror the on-chain `delegation_registry` module in the token-votes
// contract. `RegistryDelegatorInfo` is intentionally distinct from the
// pre-existing `DelegatorInfo` above (which is event-scan-derived and used by
// the legacy {@link VotesClient.getDelegators}) to avoid colliding with it.

/** A single delegation record as returned by {@link VotesClient.getReceivedDelegations}. */
export interface DelegationEntry {
  delegator: string;
  delegatee: string;
  delegatedAtLedger: number;
  votingPowerAtDelegation: bigint;
  active: boolean;
  revokedAtLedger: number | null;
}

/** One lifecycle entry (active or revoked) in a delegator's history, as returned by {@link VotesClient.getDelegationHistory}. */
export interface DelegationHistoryEntry {
  delegatee: string;
  delegatedAtLedger: number;
  revokedAtLedger: number | null;
  powerAtDelegation: bigint;
  sequence: number;
}

/** A current delegator of a delegatee, as returned by {@link VotesClient.getRegistryDelegators} and {@link VotesClient.getDelegationSnapshot}. */
export interface RegistryDelegatorInfo {
  address: string;
  delegatedPower: bigint;
  delegatedAtLedger: number;
  chainDepth: number;
}

/** Comprehensive delegate summary as returned by {@link VotesClient.getDelegateProfile}. */
export interface DelegateProfile {
  address: string;
  currentVotingPower: bigint;
  baseVotingPower: bigint;
  totalDelegators: number;
  totalDelegatedPower: bigint;
  delegationDepthLimit: number;
  firstDelegatedAtLedger: number | null;
}

/**
 * One entry of a split delegation (issue #994): a fraction of the
 * delegator's voting power, in basis points (10000 = 100%), directed to
 * `delegatee`. Mirrors `SplitDelegation` in
 * contracts/token-votes/src/split_delegation.rs.
 */
export interface SplitDelegation {
  delegatee: string;
  weightBps: number;
}

// ─── Treasury Types ───────────────────────────────────────────────────────────

/** Configuration for {@link TreasuryClient}. */
export interface TreasuryConfig {
  /** Contract address of the treasury */
  treasuryAddress: string;
  /** Stellar network to connect to */
  network: Network;
  /** RPC URL override (optional — defaults to public horizon) */
  rpcUrl?: string;
  /** Optional funded classic account used for read-only simulation calls. */
  simulationAccount?: string;
  /** Indexer base URL for off-chain queries (e.g. getBatchTransferHistory) */
  indexerUrl?: string;
  /** Maximum retry attempts for failed operations (default: 3) */
  maxAttempts?: number;
  /** Base delay between retries in milliseconds (default: 1000) */
  baseDelayMs?: number;
  /** Retry policy for all operations made by this client. */
  retry?: RetryOptions;
}

/** A single recipient in a batch transfer operation. */
export interface BatchTransferRecipient {
  /** Stellar strkey address of the recipient */
  address: string;
  /** Amount of tokens to transfer (in the token's base unit) */
  amount: bigint;
}

export interface SpendingCap {
  token: string;
  maxAmount: bigint;
  periodLedgers: number;
}

// ─── Treasury Budget Stream Types ─────────────────────────────────────────────

/** On-chain budget stream allocated to a department owner. */
export interface BudgetStream {
  id: bigint;
  name: string;
  owner: string;
  token: string;
  totalAllocated: bigint;
  totalSpent: bigint;
  startLedger: number;
  endLedger: number;
  isActive: boolean;
  isRevoked: boolean;
  revokedAtLedger: number | null;
  maxSingleSpend: bigint;
  cooldownLedgers: number;
  lastSpendLedger: number;
  spendCount: number;
  createdByProposalId: bigint;
}

/** A single spend record from a budget stream. */
export interface StreamSpend {
  streamId: bigint;
  spendIndex: number;
  recipient: string;
  amount: bigint;
  memo: string;
  executedAtLedger: number;
  executedBy: string;
}

/** Budget utilization report for a single stream. */
export interface StreamBudgetReport {
  streamId: bigint;
  name: string;
  totalAllocated: bigint;
  totalSpent: bigint;
  remaining: bigint;
  utilizationBps: number;
  isActive: boolean;
  daysRemaining: number;
  spendCount: number;
  avgSpend: bigint;
}

/** Treasury-wide budget summary across all streams. */
export interface TreasuryBudgetSummary {
  totalStreams: number;
  activeStreams: number;
  totalAllocatedByToken: Array<{ token: string; amount: bigint }>;
  totalSpentByToken: Array<{ token: string; amount: bigint }>;
  totalRemainingByToken: Array<{ token: string; amount: bigint }>;
}

/** Parameters for creating a new budget stream. */
export interface CreateStreamParams {
  name: string;
  owner: string;
  token: string;
  totalAllocated: bigint;
  startLedger: number;
  endLedger: number;
  maxSingleSpend: bigint;
  cooldownLedgers: number;
  proposalId: bigint;
}

/** Pagination options for query methods. */
export interface PaginationOptions {
  offset?: number;
  limit?: number;
}

export interface LiquidityConfig {
  /** Contract address of the liquidity pool contract */
  liquidityAddress: string;
  /** Stellar network to connect to */
  network: Network;
  /** RPC URL override (optional — defaults to public horizon) */
  rpcUrl?: string;
  /** Optional funded classic account used for read-only simulation calls. */
  simulationAccount?: string;
  /** Maximum retry attempts for failed operations (default: 3) */
  maxAttempts?: number;
  /** Base delay between retries in milliseconds (default: 1000) */
  baseDelayMs?: number;
  /** Retry policy for all operations made by this client. */
  retry?: RetryOptions;
}

/** Configuration for {@link VoteEscrowClient}. */
export interface VoteEscrowConfig {
  /** Contract address of the vote-escrow contract */
  voteEscrowAddress: string;
  /** Stellar network to connect to */
  network: Network;
  /** RPC URL override (optional — defaults to public horizon) */
  rpcUrl?: string;
  /** Optional funded classic account used for read-only simulation calls. */
  simulationAccount?: string;
  /** Maximum retry attempts for failed operations (default: 3) */
  maxAttempts?: number;
  /** Base delay between retries in milliseconds (default: 1000) */
  baseDelayMs?: number;
  /** Retry policy for all operations made by this client. */
  retry?: RetryOptions;
}

/** Configuration for {@link ConvictionVotingClient}. */
export interface ConvictionVotingConfig {
  /** Contract address of the conviction-voting contract */
  convictionVotingAddress: string;
  /** Stellar network to connect to */
  network: Network;
  /** RPC URL override (optional — defaults to public horizon) */
  rpcUrl?: string;
  /** Optional funded classic account used for read-only simulation calls. */
  simulationAccount?: string;
  /** Indexer base URL, required by the indexer-backed methods. */
  indexerUrl?: string;
  /** Maximum retry attempts for failed operations (default: 3) */
  maxAttempts?: number;
  /** Base delay between retries in milliseconds (default: 1000) */
  baseDelayMs?: number;
  /** Retry policy for all operations made by this client. */
  retry?: RetryOptions;
}

/** Configuration for {@link OptimisticGovernorClient}. */
export interface OptimisticGovernorConfig {
  /** Contract address of the optimistic-governor contract */
  optimisticGovernorAddress: string;
  /** Stellar network to connect to */
  network: Network;
  /** RPC URL override (optional — defaults to public horizon) */
  rpcUrl?: string;
  /** Optional funded classic account used for read-only simulation calls. */
  simulationAccount?: string;
  /** Indexer base URL, required by the indexer-backed methods. */
  indexerUrl?: string;
  /** Maximum retry attempts for failed operations (default: 3) */
  maxAttempts?: number;
  /** Base delay between retries in milliseconds (default: 1000) */
  baseDelayMs?: number;
  /** Retry policy for all operations made by this client. */
  retry?: RetryOptions;
}

/** On-chain state of a single two-asset liquidity pool. */
export interface Pool {
  reserveA: bigint;
  reserveB: bigint;
  totalLpSupply: bigint;
  feeBps: number;
}

/** A treasury batch transfer event as returned by the indexer. */
export interface BatchTransferEvent {
  /** SHA-256 operation hash (hex-encoded) */
  opHash: string;
  /** Strkey address of the token that was transferred */
  token: string;
  /** Number of recipients in the batch */
  recipientCount: number;
  /** Total amount transferred across all recipients */
  totalAmount: bigint;
  /** Ledger sequence number at which the transfer was executed */
  ledger: number;
}

/** Event from a budget stream lifecycle. */
export type StreamEvent =
  | {
      type: "stream_created";
      streamId: bigint;
      name: string;
      owner: string;
      ledger: number;
    }
  | {
      type: "stream_spend";
      streamId: bigint;
      recipient: string;
      amount: bigint;
      ledger: number;
    }
  | {
      type: "stream_batch";
      streamId: bigint;
      totalAmount: bigint;
      recipientCount: number;
      ledger: number;
    }
  | {
      type: "stream_revoked";
      streamId: bigint;
      caller: string;
      unspentReturned: bigint;
      ledger: number;
    }
  | {
      type: "stream_extended";
      streamId: bigint;
      oldEndLedger: number;
      newEndLedger: number;
      ledger: number;
    }
  | {
      type: "stream_topped_up";
      streamId: bigint;
      additionalAmount: bigint;
      newTotalAmount: bigint;
      ledger: number;
    }
  | {
      type: "stream_exhausted";
      streamId: bigint;
      ledger: number;
    }
  | {
      type: "stream_expired";
      streamId: bigint;
      unspent: bigint;
      ledger: number;
    };

/** Result of checking whether an address can propose. */
export interface CanProposeResult {
  /** Whether the address is allowed to propose */
  allowed: boolean;
  /** Reason code: "ok", "threshold", "cooldown", "ratelimit", "paused" */
  reason: string;
  /** Ledger when cooldown ends (if in cooldown) */
  cooldownEndsAt?: number;
  /** Number of proposals made in current period */
  proposalsThisPeriod: number;
  /** Maximum proposals allowed per period */
  maxPerPeriod: number;
  /** Current voting power of the proposer */
  votingPower: bigint;
  /** Proposal threshold required */
  threshold: bigint;
}

/** A single vote in a voter's history. */
export interface VotingHistoryEntry {
  /** Proposal ID that was voted on */
  proposalId: bigint;
  /** Vote direction (For, Against, or Abstain) */
  support: VoteSupport;
  /** Weight of the vote */
  weight: bigint;
  /** Optional reason provided with the vote */
  reason?: string;
  /** Ledger sequence when the vote was cast */
  ledger: number;
}

/**
 * Result of a `simulateProposal` dry-run.
 *
 * When `ok` is true all resource fields are populated.
 * When `ok` is false only `error` is set.
 */
export interface SimulateResult {
  /** Whether the simulation succeeded (would not revert on-chain). */
  ok: boolean;
  /** Estimated CPU instructions consumed by the propose transaction. */
  cpuInsns?: bigint;
  /** Estimated memory bytes consumed by the propose transaction. */
  memBytes?: bigint;
  /** Estimated fee in stroops required to submit the transaction. */
  feeStroops?: bigint;
  /** Human-readable error message when `ok` is false. */
  error?: string;
}

/**
 * A captured point-in-time governance activity reading (Issue #765),
 * computed periodically by the indexer from its own indexed `votes` table
 * — there's no on-chain analytics module (no room in the governor
 * contract's WASM budget alongside proposer reputation).
 */
export interface GovernanceSnapshot {
  ledger: number;
  totalVotesCast: bigint;
}

export interface AllTimeStats {
  totalProposals: bigint;
  totalVotesCast: bigint;
  uniqueVoters: bigint;
  quorumHitCount: bigint;
  quorumMissCount: bigint;
  passRateBps: number;
}

export interface VoterHistory {
  voter: string;
  proposalsVoted: number;
  proposalsEligible: number;
  participationRateBps: number;
  totalWeightCast: bigint;
  forCount: number;
  againstCount: number;
  abstainCount: number;
  lastVotedLedger: number;
}

// ─── Proposer Reputation Types (Issue #771) ──────────────────────────────────

/** On-chain proposer reputation record returned by ReputationClient.getProposerReputation. */
export interface ProposerReputation {
  proposer: string;
  totalProposals: number;
  /** Sum of participation (in basis points) across every proposal this address created. */
  totalParticipationBpsSum: bigint;
  lastProposalLedger: number;
  /** Rolling score, clamped to a fixed [-1000, 1000] range on-chain. */
  reputationScore: number;
  /** 10000 = no change, lower = discount, higher = penalty on the flat proposal threshold. */
  thresholdMultiplierBps: number;
  firstProposalLedger: number;
  consecutiveSuccessful: number;
  consecutiveFailed: number;
}

/**
 * A single entry in a proposer's reputation score history, as returned by
 * the indexer's `GET /reputation/:address/history` (built from
 * `ReputationUpdated` events — not an on-chain read, to keep the governor
 * contract's WASM size under budget).
 */
export interface ReputationScoreEntry {
  ledger: number;
  score: number;
  change: number;
  reason: string;
}

/**
 * One row of the top-proposer leaderboard, as returned by the indexer's
 * `GET /reputation/leaderboard` (computed off-chain from indexed
 * `ReputationUpdated` events, not an on-chain read).
 */
export interface ProposerLeaderboardEntry {
  rank: number;
  proposer: string;
  reputationScore: number;
  lastUpdatedLedger: number | null;
}

/**
 * Indexer-cached summary for a single proposer address, as returned by
 * `GET /reputation/:address`. Faster than an on-chain simulation for
 * display-only use cases (e.g. profile pages). The authoritative source
 * for threshold calculations is always the on-chain contract.
 */
export interface ReputationSummary {
  /** Stellar strkey address of the proposer. */
  address: string;
  /** Most-recent reputation score mirrored from `ReputationUpdated` events. */
  reputationScore: number;
  /** Ledger at which the indexer last updated this record, or null if never seen. */
  lastUpdatedLedger: number | null;
}

/**
 * Paginated page of reputation score history entries, as returned by
 * `GET /reputation/:address/history`.
 */
export interface ReputationScoreHistoryPage {
  history: ReputationScoreEntry[];
  pagination: {
    limit: number;
    offset: number;
    hasMore: boolean;
  };
}

/**
 * Paginated page of the proposer leaderboard, as returned by
 * `GET /reputation/leaderboard`.
 */
export interface ReputationLeaderboardPage {
  leaderboard: ProposerLeaderboardEntry[];
  pagination: {
    limit: number;
    offset: number;
  };
}

/**
 * A single entry in a proposer's effective-threshold history, as returned
 * by the indexer's `GET /reputation/:address/threshold-history` (built from
 * `EffectiveThresholdChanged` events).
 */
export interface ThresholdHistoryEntry {
  ledger: number;
  oldThreshold: bigint;
  newThreshold: bigint;
}

/**
 * Paginated page of threshold history entries, as returned by
 * `GET /reputation/:address/threshold-history`.
 */
export interface ThresholdHistoryPage {
  history: ThresholdHistoryEntry[];
  pagination: {
    limit: number;
    offset: number;
    hasMore: boolean;
  };
}

/**
 * Explanation for one dimension of a governance tuning recommendation, as
 * returned by the backend's `GET /governance-tuning/recommendations/*`
 * (issue #998).
 */
export interface TuningRationaleEntry {
  direction: "up" | "down" | "unchanged";
  reason: string;
}

/** Full rationale attached to a {@link TuningRecommendation}. */
export interface TuningRationale {
  quorumNumerator: TuningRationaleEntry;
  proposalThreshold: TuningRationaleEntry;
  votingPeriod: TuningRationaleEntry;
  /** Raw analytics inputs the recommendation was derived from, for auditability. */
  inputs: Record<string, unknown>;
}

/**
 * One computed governance-tuning recommendation, backend-sourced (never
 * on-chain) — see the `governance_tuning_recommendations` table.
 */
export interface TuningRecommendation {
  id: number;
  computedAt: string;
  currentQuorumNumerator: number;
  recommendedQuorumNumerator: number;
  currentProposalThreshold: bigint;
  recommendedProposalThreshold: bigint;
  rationale: TuningRationale;
  /** Whether this recommendation was automatically submitted as an on-chain proposal. */
  autoProposed: boolean;
  /** The on-chain proposal id, once/if the indexer observes a matching `ProposalCreated` event. */
  proposalId: bigint | null;
}

/** Paginated page of recommendations, as returned by `GET /governance-tuning/recommendations`. */
export interface TuningRecommendationPage {
  recommendations: TuningRecommendation[];
  pagination: {
    limit: number;
    offset: number;
    hasMore: boolean;
  };
}

/** The tunable bands/caps for the recommender, as returned by `GET /governance-tuning/config`. */
export interface TuningConfig {
  minQuorumNumerator: number;
  maxQuorumNumerator: number;
  maxQuorumDeltaBps: number;
  minProposalThreshold: bigint;
  maxProposalThreshold: bigint | null;
  maxThresholdDeltaBps: number;
  trailingWindow: number;
  intervalMs: number;
  autoPropose: boolean;
  updatedAt: string;
}

// ─── Proposal Calldata Simulation (issue #1000) ────────────────────────────

/** Result of dry-running one proposal action's calldata against current chain state. */
export interface SimulationResult {
  target: string;
  fnName: string;
  success: boolean;
  /** Best-effort human-readable description of what this action would do. */
  decodedSummary: string;
  returnValue?: unknown;
  revertReason?: string;
  treasuryImpact?: {
    token: string;
    capRemainingBefore: bigint | null;
    capRemainingAfter: bigint | null;
  };
}

/** One historical simulation run for a proposal, as returned by `GET /proposal-simulation/:id/history`. */
export interface SimulationHistoryEntry {
  simulatedAt: string;
  simulatedAtLedger: number;
  results: SimulationResult[];
  anyActionWouldRevert: boolean;
}

// ─── Voting Participation Rewards (issue #1011) ────────────────────────────

/** One reward epoch of the voting-participation rewards program. */
export interface VotingRewardsEpoch {
  id: bigint;
  /** First ledger of the epoch (inclusive). */
  startLedger: number;
  /** Ledger the epoch closes at (exclusive — the next epoch starts here). */
  endLedger: number;
  /**
   * Hex-encoded Merkle root of the epoch's `(address, amount)` set, or
   * `null` until governance publishes it.
   */
  merkleRoot: string | null;
  /** Total allocated to this epoch, in the reward token's smallest unit. */
  totalRewardAmount: bigint;
  /** How much of {@link totalRewardAmount} has been claimed so far. */
  claimedAmount: bigint;
  /** Whether the root has been published — claims only open once it is. */
  finalized: boolean;
}

/** A reward an address has earned in one epoch, with the proof needed to claim it. */
export interface ClaimableReward {
  epochId: bigint;
  amount: bigint;
  /** Hex-encoded sibling path, ready to hand straight to `claim`. */
  merkleProof: string[];
  claimed: boolean;
}

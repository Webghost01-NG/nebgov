# Co-Sponsorship Registry

## Overview

The **Co-Sponsorship** contract (`contracts/co-sponsorship`) enables token holders who lack sufficient voting power on their own to pool pledges toward meeting the Governor's `proposal_threshold`.

Rather than requiring a single proposer to meet the threshold individually, co-sponsorship introduces a **pre-proposal draft lifecycle**:
1. A creator publishes a proposal draft with actions, descriptions, and calldata.
2. Community members pledge their voting power to co-sponsor the draft.
3. Once the accumulated, live-verified co-sponsorship power meets the Governor's effective proposal threshold, the draft creator finalizes it.
4. The contract promotes the draft into a real on-chain Governor proposal via `propose_via_registry`.

## How It Works

### Pre-Proposal Draft Lifecycle

```text
Creator creates draft (below threshold)
              ↓
Community pledges via co_sponsor()
              ↓
Live voting power re-evaluated
              ↓
Threshold met? ── No ──► Draft expires at expiry_ledger (or creator/admin cancels)
      │
     Yes
      ↓
Creator calls finalize_draft()
      ↓
Governor proposal created via propose_via_registry()
```

### Key Concepts

- **Proposal Draft**: A pre-proposal record containing actions (`targets`, `fn_names`, `calldatas`), description, metadata URI, creation ledger, expiry ledger, co-sponsors list, and pledge power.
- **Co-Sponsor Power**: When an address calls `co_sponsor`, its voting power from `VotesToken` is recorded. When `finalize_draft` is called, all co-sponsors' voting powers are **re-verified live** against `VotesClient::get_votes` to ensure support hasn't lapsed or been transferred before promotion.
- **Pledge Withdrawal**: A co-sponsor may withdraw their pledge via `withdraw_co_sponsorship` at any time before the draft is finalized or cancelled.
- **Threshold Verification**: Uses the Governor's `get_effective_threshold(creator)` (which accounts for reputation adjustments, anti-spam mechanisms, and base proposal thresholds).
- **Calldata Size and Count Limits**:
  - `MAX_CALLDATA_SIZE = 10_000` bytes: Each entry in `calldatas` must not exceed 10,000 bytes.
  - `MAX_CALLDATA_COUNT = 10`: A draft cannot contain more than 10 calldata actions.

## Public Entrypoints

| Function | Signature | Description | Authorization |
| --- | --- | --- | --- |
| `initialize` | `(admin: Address, governor: Address, votes_token: Address, draft_expiry_ledgers: u32, max_co_sponsors: u32)` | Initializes registry with admin, governor, votes token, default expiry window, and max co-sponsors per draft. | `admin` |
| `create_draft` | `(creator: Address, description: String, description_hash: BytesN<32>, metadata_uri: String, targets: Vec<Address>, fn_names: Vec<Symbol>, calldatas: Vec<Bytes>) -> u64` | Creates a new pre-proposal draft and returns the generated `draft_id`. | `creator` |
| `co_sponsor` | `(sponsor: Address, draft_id: u64)` | Pledges caller's current voting power toward the draft. | `sponsor` |
| `withdraw_co_sponsorship` | `(sponsor: Address, draft_id: u64)` | Withdraws a previously pledged co-sponsorship and decreases the draft's total power. | `sponsor` |
| `finalize_draft` | `(caller: Address, draft_id: u64) -> u64` | Verifies live co-sponsor power against governor's effective threshold and promotes draft into a proposal. | `caller` (must be `draft.creator`) |
| `cancel_draft` | `(caller: Address, draft_id: u64)` | Cancels an open draft. | `caller` (must be `draft.creator` or `admin`) |
| `get_draft` | `(draft_id: u64) -> ProposalDraft` | Reads a proposal draft by ID; emits `DraftExpired` lazily if the draft has passed its expiry ledger. | None (read-only) |
| `get_active_drafts` | `(offset: u64, limit: u64) -> Vec<ProposalDraft>` | Returns a paginated list of drafts that are not finalized, cancelled, or expired. | None (read-only) |
| `get_co_sponsor_power` | `(draft_id: u64, sponsor: Address) -> i128` | Reads the recorded pledged voting power for a specific sponsor on a draft. | None (read-only) |
| `draft_threshold_met` | `(draft_id: u64) -> bool` | Checks whether the draft's total power meets the governor's base proposal threshold. | None (read-only) |
| `check_not_initialized` | `()` | Helper that verifies initialization and panics with `NotInitialized` if uninitialized. | None |
| `check_index_out_of_bounds` | `()` | Helper that panics with `IndexOutOfBounds` for verification of error typing. | None |
| `set_governor` | `(governor: Address)` | Updates the governor contract address. | `admin` |
| `set_votes_token` | `(votes_token: Address)` | Updates the votes token contract address. | `admin` |
| `set_draft_expiry_ledgers` | `(draft_expiry_ledgers: u32)` | Updates the default lifetime (in ledgers) for newly created drafts. | `admin` |
| `set_max_co_sponsors` | `(max_co_sponsors: u32)` | Updates the maximum allowed co-sponsors per draft. | `admin` |

## Events

The contract publishes structured events for indexers, frontends, and client SDKs:

| Topic | Data Payload | Emitted When |
| --- | --- | --- |
| `("DraftCreated", creator: Address)` | `DraftCreatedEvent { draft_id, creator, description_hash, metadata_uri, created_ledger, expiry_ledger }` | A new draft pre-proposal is created via `create_draft`. |
| `("CoSponsored", sponsor: Address)` | `(draft_id: u64, power: i128, total_power: i128)` | An address pledges voting power to a draft via `co_sponsor`. |
| `("CoSponsorshipWithdrawn", sponsor: Address)` | `(draft_id: u64, power: i128, total_power: i128)` | A sponsor removes their pledge via `withdraw_co_sponsorship`. |
| `("DraftFinalized",)` | `(draft_id: u64, proposal_id: u64)` | Draft meets threshold and promotes to proposal via `finalize_draft`. |
| `("DraftCancelled",)` | `(draft_id: u64, caller: Address)` | Draft is cancelled by its creator or admin via `cancel_draft`. |
| `("DraftExpired",)` | `(draft_id: u64, expired_at_ledger: u32)` | Draft is read via `get_draft` after its `expiry_ledger` has passed. |

## Errors

`CoSponsorshipError` is a `#[contracterror]` with explicit status codes:

| Code | Name | Description |
| --- | --- | --- |
| 1 | `AlreadyInitialized` | Attempted to initialize an already initialized contract. |
| 2 | `DraftNotFound` | The requested draft ID does not exist in storage. |
| 3 | `DraftExpired` | The draft's expiry ledger has elapsed. |
| 4 | `DraftClosed` | The draft is already finalized or cancelled. |
| 5 | `AlreadyCoSponsored` | The caller has already pledged power to this draft. |
| 6 | `NotCoSponsored` | The caller has not pledged power to this draft. |
| 7 | `CoSponsorLimitReached` | The draft has reached `MaxCoSponsors` pledges. |
| 8 | `DraftThresholdNotMet` | Accumulated live co-sponsor power is less than the Governor's effective threshold. |
| 9 | `UnauthorizedDraftCreator` | Caller is not the creator (or admin, for cancellation). |
| 10 | `ZeroVotingPower` | Caller has zero or negative voting power at pledge time. |
| 11 | `InvalidVectorLengths` | Lengths of `targets`, `fn_names`, and `calldatas` do not match. |
| 12 | `NoTargets` | Targets list is empty. |
| 13 | `CalldataTooLarge` | A calldata payload exceeds `MAX_CALLDATA_SIZE` (10,000 bytes). |
| 14 | `TooManyCalldataEntries` | Number of calldata entries exceeds `MAX_CALLDATA_COUNT` (10). |
| 15 | `NotInitialized` | Contract instance storage has not been initialized. |
| 16 | `IndexOutOfBounds` | Internal vector indexing encountered an out-of-bounds index. |

## Deployment Order

The Co-Sponsorship contract integrates with both `token-votes` and `governor`. To deploy and connect correctly:

1. **Deploy and initialize Token-Votes**: Deploy `contracts/token-votes` (or wrap a SEP-41 token via `contracts/token-votes-wrapper`).
2. **Deploy and initialize Timelock**: Deploy `contracts/timelock`.
3. **Deploy and initialize Governor**: Deploy `contracts/governor`, configuring it with the token-votes and timelock contract addresses.
4. **Deploy and initialize Co-Sponsorship**: Deploy `contracts/co-sponsorship`, calling:
   ```text
   initialize(admin, governor_address, votes_token_address, draft_expiry_ledgers, max_co_sponsors)
   ```
   Typical testnet parameters: `draft_expiry_ledgers = 7200` (~10 hours at 5s ledgers), `max_co_sponsors = 20`.
5. **Authorize Registry on Governor**: In `contracts/governor`, ensure the co-sponsorship contract address is authorized to call `propose_via_registry`.

## Related Documentation

- [Architecture Overview](./architecture.md)
- [Governor Lifecycle](./proposal-simulation.md)
- [Vote Escrow](./vote-escrow.md)
- [Troubleshooting](./troubleshooting.md)

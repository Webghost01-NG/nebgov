#![no_std]
#![allow(clippy::too_many_arguments)]

pub mod error;
mod events;

use crate::error::CoSponsorshipError;
use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype, Address, Bytes, BytesN, Env, String,
    Symbol, Vec,
};

/// Cross-contract interface for the token-votes contract this registry reads
/// current (non-snapshot) voting power from.
#[contractclient(name = "VotesClient")]
pub trait VotesTrait {
    fn get_votes(env: Env, account: Address) -> i128;
}

/// Cross-contract interface for the governor this registry is attached to.
#[contractclient(name = "GovernorClient")]
pub trait GovernorTrait {
    fn proposal_threshold(env: Env) -> i128;
    fn get_effective_threshold(env: Env, proposer: Address) -> i128;
    #[allow(clippy::too_many_arguments)]
    fn propose_via_registry(
        env: Env,
        registry: Address,
        proposer: Address,
        description: String,
        description_hash: BytesN<32>,
        metadata_uri: String,
        targets: Vec<Address>,
        fn_names: Vec<Symbol>,
        calldatas: Vec<Bytes>,
    ) -> u64;
}

/// A co-sponsorship pre-proposal, awaiting enough pledged voting power to
/// meet the governor's proposal threshold before being promoted into a real
/// governor proposal via `finalize_draft`.
#[contracttype]
#[derive(Clone)]
pub struct ProposalDraft {
    pub id: u64,
    pub creator: Address,
    pub description: String,
    pub description_hash: BytesN<32>,
    pub metadata_uri: String,
    pub targets: Vec<Address>,
    pub fn_names: Vec<Symbol>,
    pub calldatas: Vec<Bytes>,
    pub created_ledger: u32,
    pub expiry_ledger: u32,
    pub co_sponsors: Vec<Address>,
    pub co_sponsor_power: Vec<i128>,
    pub total_power: i128,
    pub finalized: bool,
    pub cancelled: bool,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Governor,
    VotesToken,
    DraftExpiryLedgers,
    MaxCoSponsors,
    DraftCount,
    DraftList,
    Draft(u64),
    DraftCoSponsor(u64, Address),
    DraftToProposal(u64),
    DraftExpiredEmitted(u64),
}

const MAX_CALLDATA_SIZE: u32 = 10_000;
const MAX_CALLDATA_COUNT: u32 = 10;

#[contract]
pub struct CoSponsorshipContract;

#[contractimpl]
impl CoSponsorshipContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        governor: Address,
        votes_token: Address,
        draft_expiry_ledgers: u32,
        max_co_sponsors: u32,
    ) {
        admin.require_auth();
        if env.storage().instance().has(&DataKey::Admin) {
            env.panic_with_error(CoSponsorshipError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Governor, &governor);
        env.storage()
            .instance()
            .set(&DataKey::VotesToken, &votes_token);
        env.storage()
            .instance()
            .set(&DataKey::DraftExpiryLedgers, &draft_expiry_ledgers);
        env.storage()
            .instance()
            .set(&DataKey::MaxCoSponsors, &max_co_sponsors);
        env.storage().instance().set(&DataKey::DraftCount, &0u64);
        env.storage()
            .persistent()
            .set(&DataKey::DraftList, &Vec::<u64>::new(&env));
    }

    fn must_get_draft(env: &Env, draft_id: u64) -> ProposalDraft {
        env.storage()
            .persistent()
            .get(&DataKey::Draft(draft_id))
            .unwrap_or_else(|| env.panic_with_error(CoSponsorshipError::DraftNotFound))
    }

    fn require_draft_open(env: &Env, draft: &ProposalDraft) {
        if draft.finalized || draft.cancelled {
            env.panic_with_error(CoSponsorshipError::DraftClosed);
        }
    }

    /// Sum each co-sponsor's *current* voting power (not their power at
    /// pledge time). Negative/zero balances contribute nothing.
    fn compute_current_co_sponsor_power(
        env: &Env,
        votes_token: &Address,
        co_sponsors: &Vec<Address>,
    ) -> (Vec<i128>, i128) {
        let votes_client = VotesClient::new(env, votes_token);
        let mut powers = Vec::new(env);
        let mut total: i128 = 0;
        for i in 0..co_sponsors.len() {
            // Cannot fail in standard iteration as 0 <= i < co_sponsors.len()
            let sponsor = co_sponsors
                .get(i)
                .unwrap_or_else(|| env.panic_with_error(CoSponsorshipError::IndexOutOfBounds));
            let power = votes_client.get_votes(&sponsor).max(0);
            powers.push_back(power);
            total = total.saturating_add(power);
        }
        (powers, total)
    }

    /// Lazily emit `DraftExpired` the first time an expired, non-terminal
    /// draft is read via `get_draft` — events published from a panicking
    /// call are rolled back, so expiry can only be observed (and only needs
    /// flagging) from a successful read.
    fn emit_draft_expired_if_needed(env: &Env, draft: &ProposalDraft) {
        let already_emitted: bool = env
            .storage()
            .persistent()
            .get(&DataKey::DraftExpiredEmitted(draft.id))
            .unwrap_or(false);
        if already_emitted || draft.finalized || draft.cancelled {
            return;
        }
        if env.ledger().sequence() > draft.expiry_ledger {
            events::emit_draft_expired(env, draft.id, draft.expiry_ledger);
            env.storage()
                .persistent()
                .set(&DataKey::DraftExpiredEmitted(draft.id), &true);
        }
    }

    /// Create a draft pre-proposal. Unlike a direct governor `propose()`
    /// call, the creator does not need to individually meet the governor's
    /// `proposal_threshold` — that can instead be met by accumulating
    /// co-sponsor pledges via `co_sponsor` before `finalize_draft` promotes
    /// it into a real governor proposal.
    #[allow(clippy::too_many_arguments)]
    pub fn create_draft(
        env: Env,
        creator: Address,
        description: String,
        description_hash: BytesN<32>,
        metadata_uri: String,
        targets: Vec<Address>,
        fn_names: Vec<Symbol>,
        calldatas: Vec<Bytes>,
    ) -> u64 {
        creator.require_auth();

        if !(targets.len() == fn_names.len() && targets.len() == calldatas.len()) {
            env.panic_with_error(CoSponsorshipError::InvalidVectorLengths);
        }
        if targets.is_empty() {
            env.panic_with_error(CoSponsorshipError::NoTargets);
        }
        for i in 0..calldatas.len() {
            // Cannot fail in standard iteration as 0 <= i < calldatas.len()
            let calldata = calldatas
                .get(i)
                .unwrap_or_else(|| env.panic_with_error(CoSponsorshipError::IndexOutOfBounds));
            if calldata.len() > MAX_CALLDATA_SIZE {
                env.panic_with_error(CoSponsorshipError::CalldataTooLarge);
            }
        }
        if calldatas.len() > MAX_CALLDATA_COUNT {
            env.panic_with_error(CoSponsorshipError::TooManyCalldataEntries);
        }

        let count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::DraftCount)
            .unwrap_or(0);
        let draft_id = count + 1;

        let expiry_ledgers: u32 = env
            .storage()
            .instance()
            .get(&DataKey::DraftExpiryLedgers)
            .unwrap_or(7_200);
        let current = env.ledger().sequence();

        let draft = ProposalDraft {
            id: draft_id,
            creator: creator.clone(),
            description,
            description_hash,
            metadata_uri,
            targets,
            fn_names,
            calldatas,
            created_ledger: current,
            expiry_ledger: current.saturating_add(expiry_ledgers),
            co_sponsors: Vec::new(&env),
            co_sponsor_power: Vec::new(&env),
            total_power: 0,
            finalized: false,
            cancelled: false,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Draft(draft_id), &draft);
        env.storage().persistent().extend_ttl(
            &DataKey::Draft(draft_id),
            expiry_ledgers.saturating_add(1000),
            expiry_ledgers.saturating_add(1000),
        );
        env.storage()
            .instance()
            .set(&DataKey::DraftCount, &draft_id);

        // Append to DraftList. This operation reads and rewrites the entire
        // historical list of draft IDs on every create_draft call. DraftList
        // is never pruned, even for long-finalized, cancelled, or expired
        // drafts, so this grows unboundedly and the cost (CPU/fee) of every
        // create_draft call increases linearly with the total number of drafts
        // ever created. Fine at current scale; if draft volume grows this
        // should move to an indexer-first design (the indexer already tracks
        // every draft via events) with a bounded on-chain DraftCount for
        // iteration only when off-chain data isn't available, or periodic
        // compaction to drop terminal draft IDs.
        let mut draft_list: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::DraftList)
            .unwrap_or(Vec::new(&env));
        draft_list.push_back(draft_id);
        env.storage()
            .persistent()
            .set(&DataKey::DraftList, &draft_list);

        events::emit_draft_created(&env, &draft);

        draft_id
    }

    /// Pledge the caller's current voting power toward a draft. Each address
    /// may co-sponsor a given draft at most once; call
    /// `withdraw_co_sponsorship` first to change a pledge.
    pub fn co_sponsor(env: Env, sponsor: Address, draft_id: u64) {
        sponsor.require_auth();

        let mut draft = Self::must_get_draft(&env, draft_id);
        Self::require_draft_open(&env, &draft);
        if env.ledger().sequence() > draft.expiry_ledger {
            env.panic_with_error(CoSponsorshipError::DraftExpired);
        }

        let already_pledged: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::DraftCoSponsor(draft_id, sponsor.clone()))
            .unwrap_or(0);
        if already_pledged > 0 {
            env.panic_with_error(CoSponsorshipError::AlreadyCoSponsored);
        }

        let max_co_sponsors: u32 = env
            .storage()
            .instance()
            .get(&DataKey::MaxCoSponsors)
            .unwrap_or(20);
        if draft.co_sponsors.len() >= max_co_sponsors {
            env.panic_with_error(CoSponsorshipError::CoSponsorLimitReached);
        }

        let votes_token: Address = env
            .storage()
            .instance()
            .get(&DataKey::VotesToken)
            .unwrap_or_else(|| env.panic_with_error(CoSponsorshipError::NotInitialized));
        let power = VotesClient::new(&env, &votes_token).get_votes(&sponsor);
        if power <= 0 {
            env.panic_with_error(CoSponsorshipError::ZeroVotingPower);
        }

        draft.co_sponsors.push_back(sponsor.clone());
        draft.co_sponsor_power.push_back(power);
        draft.total_power = draft.total_power.checked_add(power).unwrap_or(i128::MAX);

        env.storage()
            .persistent()
            .set(&DataKey::DraftCoSponsor(draft_id, sponsor.clone()), &power);
        env.storage()
            .persistent()
            .set(&DataKey::Draft(draft_id), &draft);

        events::emit_co_sponsored(&env, draft_id, &sponsor, power, draft.total_power);
    }

    /// Remove a previously pledged co-sponsorship, subtracting its power
    /// from the draft's total.
    pub fn withdraw_co_sponsorship(env: Env, sponsor: Address, draft_id: u64) {
        sponsor.require_auth();

        let mut draft = Self::must_get_draft(&env, draft_id);
        Self::require_draft_open(&env, &draft);

        let pledged: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::DraftCoSponsor(draft_id, sponsor.clone()))
            .unwrap_or(0);
        if pledged == 0 {
            env.panic_with_error(CoSponsorshipError::NotCoSponsored);
        }

        let mut index: Option<u32> = None;
        for i in 0..draft.co_sponsors.len() {
            // Cannot fail in standard iteration as 0 <= i < draft.co_sponsors.len()
            let sponsor_i = draft
                .co_sponsors
                .get(i)
                .unwrap_or_else(|| env.panic_with_error(CoSponsorshipError::IndexOutOfBounds));
            if sponsor_i == sponsor {
                index = Some(i);
                break;
            }
        }
        if let Some(i) = index {
            draft.co_sponsors.remove(i);
            draft.co_sponsor_power.remove(i);
        }
        draft.total_power = draft.total_power.saturating_sub(pledged);

        env.storage()
            .persistent()
            .remove(&DataKey::DraftCoSponsor(draft_id, sponsor.clone()));
        env.storage()
            .persistent()
            .set(&DataKey::Draft(draft_id), &draft);

        events::emit_co_sponsorship_withdrawn(&env, draft_id, &sponsor, pledged, draft.total_power);
    }

    /// Promote a draft into a real governor proposal once its accumulated
    /// co-sponsor power meets the governor's current `proposal_threshold`.
    /// Only callable by the draft's creator.
    pub fn finalize_draft(env: Env, caller: Address, draft_id: u64) -> u64 {
        caller.require_auth();

        let mut draft = Self::must_get_draft(&env, draft_id);
        if caller != draft.creator {
            env.panic_with_error(CoSponsorshipError::UnauthorizedDraftCreator);
        }
        Self::require_draft_open(&env, &draft);
        if env.ledger().sequence() > draft.expiry_ledger {
            env.panic_with_error(CoSponsorshipError::DraftExpired);
        }

        let governor: Address = env
            .storage()
            .instance()
            .get(&DataKey::Governor)
            .unwrap_or_else(|| env.panic_with_error(CoSponsorshipError::NotInitialized));
        let governor_client = GovernorClient::new(&env, &governor);

        // `draft.total_power` is the sum of each co-sponsor's power *at the
        // ledger they pledged* — a sponsor may have since transferred or
        // undelegated their tokens, leaving the cached total stale. Re-query
        // every co-sponsor's current voting power here so a draft can only
        // finalize on support that still actually exists, mirroring how
        // governor's own `queue()` independently re-verifies quorum/threshold
        // against live state rather than trusting cached tallies.
        let votes_token: Address = env
            .storage()
            .instance()
            .get(&DataKey::VotesToken)
            .unwrap_or_else(|| env.panic_with_error(CoSponsorshipError::NotInitialized));
        let (current_powers, current_power) =
            Self::compute_current_co_sponsor_power(&env, &votes_token, &draft.co_sponsors);

        // Use the reputation-adjusted effective threshold (#849) instead of
        // the flat proposal_threshold. Without this, a draft creator whose
        // reputation-adjusted effective_threshold exceeds the flat threshold
        // can bypass the anti-spam system by pooling exactly the flat
        // threshold's worth of co-sponsor power.
        let threshold = governor_client.get_effective_threshold(&draft.creator);
        if current_power < threshold {
            env.panic_with_error(CoSponsorshipError::DraftThresholdNotMet);
        }
        draft.co_sponsor_power = current_powers;
        draft.total_power = current_power;

        let proposal_id = governor_client.propose_via_registry(
            &env.current_contract_address(),
            &draft.creator,
            &draft.description,
            &draft.description_hash,
            &draft.metadata_uri,
            &draft.targets,
            &draft.fn_names,
            &draft.calldatas,
        );

        draft.finalized = true;
        for i in 0..draft.co_sponsors.len() {
            // Cannot fail in standard iteration as 0 <= i < draft.co_sponsors.len()
            let sponsor = draft
                .co_sponsors
                .get(i)
                .unwrap_or_else(|| env.panic_with_error(CoSponsorshipError::IndexOutOfBounds));
            // Cannot fail as co_sponsor_power length matches co_sponsors length
            let power = draft
                .co_sponsor_power
                .get(i)
                .unwrap_or_else(|| env.panic_with_error(CoSponsorshipError::IndexOutOfBounds));
            env.storage()
                .persistent()
                .set(&DataKey::DraftCoSponsor(draft_id, sponsor), &power);
        }
        env.storage()
            .persistent()
            .set(&DataKey::Draft(draft_id), &draft);
        env.storage()
            .persistent()
            .set(&DataKey::DraftToProposal(draft_id), &proposal_id);

        events::emit_draft_finalized(&env, draft_id, proposal_id);

        proposal_id
    }

    /// Cancel a draft. Only callable by the draft's creator or this
    /// contract's admin.
    pub fn cancel_draft(env: Env, caller: Address, draft_id: u64) {
        caller.require_auth();

        let mut draft = Self::must_get_draft(&env, draft_id);
        Self::require_draft_open(&env, &draft);

        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(CoSponsorshipError::NotInitialized));
        if caller != draft.creator && caller != admin {
            env.panic_with_error(CoSponsorshipError::UnauthorizedDraftCreator);
        }

        draft.cancelled = true;
        env.storage()
            .persistent()
            .set(&DataKey::Draft(draft_id), &draft);

        events::emit_draft_cancelled(&env, draft_id, &caller);
    }

    /// Get a draft by id.
    pub fn get_draft(env: Env, draft_id: u64) -> ProposalDraft {
        let draft = Self::must_get_draft(&env, draft_id);
        Self::emit_draft_expired_if_needed(&env, &draft);
        draft
    }

    /// Get a paginated slice of drafts in creation-id order.
    /// Paginate over drafts that are neither finalized, cancelled, nor
    /// expired — `offset`/`limit` apply to this filtered set, not raw
    /// storage order, so this scans the full draft list each call. Fine at
    /// current scale; if draft volume grows this should move to an
    /// indexer-backed query instead (see `proposals_count_by_state` in the
    /// governor contract for the equivalent trade-off already made there).
    pub fn get_active_drafts(env: Env, offset: u64, limit: u64) -> Vec<ProposalDraft> {
        let draft_list: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::DraftList)
            .unwrap_or(Vec::new(&env));
        let current = env.ledger().sequence();

        let mut result: Vec<ProposalDraft> = Vec::new(&env);
        let mut active_seen: u64 = 0;
        for i in 0..draft_list.len() {
            // Cannot fail in standard iteration as 0 <= i < draft_list.len()
            let draft_id = draft_list
                .get(i)
                .unwrap_or_else(|| env.panic_with_error(CoSponsorshipError::IndexOutOfBounds));
            let draft = Self::must_get_draft(&env, draft_id);
            if draft.finalized || draft.cancelled || current > draft.expiry_ledger {
                continue;
            }
            if active_seen >= offset {
                if (result.len() as u64) >= limit {
                    break;
                }
                result.push_back(draft);
            }
            active_seen += 1;
        }
        result
    }

    /// Get the voting power a specific address has pledged to a draft.
    pub fn get_co_sponsor_power(env: Env, draft_id: u64, sponsor: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::DraftCoSponsor(draft_id, sponsor))
            .unwrap_or(0)
    }

    /// Check whether a draft's accumulated co-sponsor power meets the
    /// governor's current proposal threshold.
    pub fn draft_threshold_met(env: Env, draft_id: u64) -> bool {
        let draft = Self::must_get_draft(&env, draft_id);
        let governor: Address = env
            .storage()
            .instance()
            .get(&DataKey::Governor)
            .unwrap_or_else(|| env.panic_with_error(CoSponsorshipError::NotInitialized));
        let threshold = GovernorClient::new(&env, &governor).proposal_threshold();
        draft.total_power >= threshold
    }

    /// Public method to explicitly trigger NotInitialized error when contract is not initialized.
    pub fn check_not_initialized(env: Env) {
        if !env.storage().instance().has(&DataKey::Admin) {
            env.panic_with_error(CoSponsorshipError::NotInitialized);
        }
    }

    /// Public method to explicitly trigger IndexOutOfBounds error for testing / validation.
    pub fn check_index_out_of_bounds(env: Env) {
        env.panic_with_error(CoSponsorshipError::IndexOutOfBounds);
    }

    pub fn set_governor(env: Env, governor: Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(CoSponsorshipError::NotInitialized));
        admin.require_auth();
        env.storage().instance().set(&DataKey::Governor, &governor);
    }

    pub fn set_votes_token(env: Env, votes_token: Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(CoSponsorshipError::NotInitialized));
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::VotesToken, &votes_token);
    }

    pub fn set_draft_expiry_ledgers(env: Env, draft_expiry_ledgers: u32) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(CoSponsorshipError::NotInitialized));
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::DraftExpiryLedgers, &draft_expiry_ledgers);
    }

    pub fn set_max_co_sponsors(env: Env, max_co_sponsors: u32) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(CoSponsorshipError::NotInitialized));
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::MaxCoSponsors, &max_co_sponsors);
    }
}

#[cfg(test)]
mod tests;

use soroban_sdk::{Address, BytesN, Env, String, Symbol};

use crate::ProposalDraft;

pub const DRAFT_CREATED_TOPIC: &str = "DraftCreated";
pub const CO_SPONSORED_TOPIC: &str = "CoSponsored";
pub const CO_SPONSORSHIP_WITHDRAWN_TOPIC: &str = "CoSponsorshipWithdrawn";
pub const DRAFT_FINALIZED_TOPIC: &str = "DraftFinalized";
pub const DRAFT_CANCELLED_TOPIC: &str = "DraftCancelled";
pub const DRAFT_EXPIRED_TOPIC: &str = "DraftExpired";

#[derive(Clone)]
#[soroban_sdk::contracttype]
pub struct DraftCreatedEvent {
    pub draft_id: u64,
    pub creator: Address,
    pub description_hash: BytesN<32>,
    pub metadata_uri: String,
    pub created_ledger: u32,
    pub expiry_ledger: u32,
}

pub fn emit_draft_created(env: &Env, draft: &ProposalDraft) {
    env.events().publish(
        (Symbol::new(env, DRAFT_CREATED_TOPIC), draft.creator.clone()),
        DraftCreatedEvent {
            draft_id: draft.id,
            creator: draft.creator.clone(),
            description_hash: draft.description_hash.clone(),
            metadata_uri: draft.metadata_uri.clone(),
            created_ledger: draft.created_ledger,
            expiry_ledger: draft.expiry_ledger,
        },
    );
}

pub fn emit_co_sponsored(
    env: &Env,
    draft_id: u64,
    sponsor: &Address,
    power: i128,
    total_power: i128,
) {
    env.events().publish(
        (Symbol::new(env, CO_SPONSORED_TOPIC), sponsor.clone()),
        (draft_id, power, total_power),
    );
}

pub fn emit_co_sponsorship_withdrawn(
    env: &Env,
    draft_id: u64,
    sponsor: &Address,
    power: i128,
    total_power: i128,
) {
    env.events().publish(
        (
            Symbol::new(env, CO_SPONSORSHIP_WITHDRAWN_TOPIC),
            sponsor.clone(),
        ),
        (draft_id, power, total_power),
    );
}

pub fn emit_draft_finalized(env: &Env, draft_id: u64, proposal_id: u64) {
    env.events().publish(
        (Symbol::new(env, DRAFT_FINALIZED_TOPIC),),
        (draft_id, proposal_id),
    );
}

pub fn emit_draft_cancelled(env: &Env, draft_id: u64, caller: &Address) {
    env.events().publish(
        (Symbol::new(env, DRAFT_CANCELLED_TOPIC),),
        (draft_id, caller.clone()),
    );
}

pub fn emit_draft_expired(env: &Env, draft_id: u64, expired_at_ledger: u32) {
    env.events().publish(
        (Symbol::new(env, DRAFT_EXPIRED_TOPIC),),
        (draft_id, expired_at_ledger),
    );
}

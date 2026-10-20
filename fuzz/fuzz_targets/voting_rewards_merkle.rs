//! Fuzz test driving random epoch and claim sequences in voting-rewards.
//!
//! Invariants asserted:
//! 1. A leaf can be claimed at most once.
//! 2. Claimed totals never exceed the epoch's total reward or the overall pool balance.

#![no_main]

use std::vec::Vec;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient,
    Address, Bytes, BytesN, Env, Vec as SorobanVec,
};
use sorogov_voting_rewards::{
    merkle::compute_leaf, VotingRewardsContract, VotingRewardsContractClient,
};

/// Helper to hash pair matching voting-rewards internal node hashing
fn hash_pair(env: &Env, a: &BytesN<32>, b: &BytesN<32>) -> BytesN<32> {
    let (first, second) = if a.to_array() <= b.to_array() {
        (a, b)
    } else {
        (b, a)
    };
    let mut buf = Bytes::new(env);
    buf.append(&Bytes::from_array(env, &first.to_array()));
    buf.append(&Bytes::from_array(env, &second.to_array()));
    env.crypto().sha256(&buf).into()
}

#[derive(Arbitrary, Debug)]
enum Action {
    FundPool {
        amount: i64,
    },
    AdvanceLedgers {
        delta: u16,
    },
    StartNextEpoch,
    PublishEpochRoot {
        reward_amount: i64,
    },
    Claim {
        voter_idx: u8,
        epoch_id: u64,
        tamper_amount: bool,
    },
    SweepEpoch {
        epoch_id: u64,
    },
}

#[derive(Arbitrary, Debug)]
struct FuzzInput {
    epoch_duration_ledgers: u16,
    initial_pool_fund: i64,
    actions: Vec<Action>,
}

fuzz_target!(|input: FuzzInput| {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let reward_token = sac.address();
    let token_admin = StellarAssetClient::new(&env, &reward_token);

    let duration = (input.epoch_duration_ledgers as u32).max(10);
    let contract_id = env.register(VotingRewardsContract, ());
    let client = VotingRewardsContractClient::new(&env, &contract_id);
    client.initialize(&admin, &reward_token, &duration);

    let voters = [
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    ];

    let funder = Address::generate(&env);
    let initial_fund = (input.initial_pool_fund as i128).abs().min(100_000_000_000);
    if initial_fund > 0 {
        token_admin.mint(&funder, &initial_fund);
        let _ = client.try_fund_pool(&funder, &initial_fund);
    }

    struct PublishedClaim {
        epoch_id: u64,
        voter: Address,
        amount: i128,
        proof: SorobanVec<BytesN<32>>,
    }
    let mut published_claims: Vec<PublishedClaim> = Vec::new();

    for action in input.actions.into_iter().take(40) {
        match action {
            Action::FundPool { amount } => {
                let amt = (amount as i128).abs().min(10_000_000);
                if amt > 0 {
                    token_admin.mint(&funder, &amt);
                    let _ = client.try_fund_pool(&funder, &amt);
                }
            }
            Action::AdvanceLedgers { delta } => {
                let current = env.ledger().sequence();
                env.ledger()
                    .set_sequence_number(current.saturating_add(delta as u32));
            }
            Action::StartNextEpoch => {
                let _ = client.try_start_next_epoch();
            }
            Action::PublishEpochRoot { reward_amount } => {
                let epoch_id = client.get_current_epoch_id();
                if let Some(epoch) = client.get_epoch(&epoch_id) {
                    if !epoch.finalized && env.ledger().sequence() >= epoch.end_ledger {
                        let total_alloc = (reward_amount as i128).abs().min(1_000_000);
                        if total_alloc > 0 && total_alloc <= client.get_available_pool() {
                            // Split allocation between voters[0] and voters[1]
                            let share_0 = total_alloc / 2;
                            let share_1 = total_alloc - share_0;

                            let leaf0 = compute_leaf(&env, &voters[0], epoch_id, share_0);
                            let leaf1 = compute_leaf(&env, &voters[1], epoch_id, share_1);
                            let root = hash_pair(&env, &leaf0, &leaf1);

                            let mut proof0 = SorobanVec::new(&env);
                            proof0.push_back(leaf1.clone());

                            let mut proof1 = SorobanVec::new(&env);
                            proof1.push_back(leaf0.clone());

                            let res = client.try_publish_epoch_root(
                                &admin,
                                &epoch_id,
                                &root,
                                &total_alloc,
                            );
                            if let Ok(Ok(())) = res {
                                published_claims.push(PublishedClaim {
                                    epoch_id,
                                    voter: voters[0].clone(),
                                    amount: share_0,
                                    proof: proof0,
                                });
                                published_claims.push(PublishedClaim {
                                    epoch_id,
                                    voter: voters[1].clone(),
                                    amount: share_1,
                                    proof: proof1,
                                });
                            }
                        }
                    }
                }
            }
            Action::Claim {
                voter_idx,
                epoch_id,
                tamper_amount,
            } => {
                let candidate = published_claims.iter().find(|c| {
                    c.epoch_id == epoch_id && c.voter == voters[voter_idx as usize % voters.len()]
                });

                if let Some(pc) = candidate {
                    let claim_amt = if tamper_amount {
                        pc.amount.saturating_add(1)
                    } else {
                        pc.amount
                    };
                    let already_claimed_before = client.has_claimed(&pc.epoch_id, &pc.voter);
                    let res = client.try_claim(&pc.voter, &pc.epoch_id, &claim_amt, &pc.proof);

                    // --- Invariant 1: A leaf can be claimed at most once ---
                    if already_claimed_before {
                        assert!(
                            res.is_err() || res.unwrap().is_err(),
                            "Double claim must be rejected for voter {:?} in epoch {}",
                            pc.voter,
                            pc.epoch_id
                        );
                    }
                } else {
                    let voter = &voters[voter_idx as usize % voters.len()];
                    let _ = client.try_claim(voter, &epoch_id, &100, &SorobanVec::new(&env));
                }
            }
            Action::SweepEpoch { epoch_id } => {
                let _ = client.try_sweep_epoch(&admin, &epoch_id);
            }
        }

        // --- Invariant 2: Claimed totals never exceed the pool or epoch reward ---
        let current_epoch_id = client.get_current_epoch_id();
        for eid in 0..=current_epoch_id {
            if let Some(epoch) = client.get_epoch(&eid) {
                assert!(
                    epoch.claimed_amount <= epoch.total_reward_amount,
                    "Epoch {} claimed amount {} exceeded total reward {}",
                    eid,
                    epoch.claimed_amount,
                    epoch.total_reward_amount
                );
                assert!(
                    epoch.claimed_amount >= 0,
                    "Epoch {} claimed amount must not be negative: {}",
                    eid,
                    epoch.claimed_amount
                );
            }
        }

        let available_pool = client.get_available_pool();
        assert!(
            available_pool >= 0,
            "Available pool must not be negative: {}",
            available_pool
        );
    }
});

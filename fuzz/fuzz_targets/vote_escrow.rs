//! Fuzz test driving random sequences of public entrypoints in vote-escrow.
//!
//! Invariants asserted:
//! 1. Locked amounts (per-lock and total_locked) never go negative.
//! 2. Time decay never increases voting power.

#![no_main]

use std::vec::Vec;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient,
    Address, Env,
};
use sorogov_vote_escrow::{VoteEscrowContract, VoteEscrowContractClient};

#[derive(Arbitrary, Debug)]
enum Action {
    CreateLock {
        user_idx: u8,
        amount: i64,
        duration_ledgers: u32,
    },
    IncreaseAmount {
        user_idx: u8,
        additional_amount: i64,
    },
    ExtendDuration {
        user_idx: u8,
        additional_duration: u32,
    },
    Withdraw {
        user_idx: u8,
    },
    AdvanceLedgers {
        delta: u16,
    },
}

#[derive(Arbitrary, Debug)]
struct FuzzInput {
    min_lock_duration: u16,
    max_lock_duration: u32,
    max_multiplier_bps: u32,
    actions: Vec<Action>,
}

fuzz_target!(|input: FuzzInput| {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token = sac.address();
    let token_admin = StellarAssetClient::new(&env, &token);

    let min_duration = (input.min_lock_duration as u32).max(10);
    let max_duration = input
        .max_lock_duration
        .max(min_duration + 10)
        .min(10_000_000);
    let max_multiplier = input.max_multiplier_bps.min(100_000);

    let contract_id = env.register(VoteEscrowContract, ());
    let client = VoteEscrowContractClient::new(&env, &contract_id);
    client.initialize(
        &admin,
        &token,
        &min_duration,
        &max_duration,
        &max_multiplier,
    );

    let users = [
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    ];
    for u in &users {
        token_admin.mint(u, &1_000_000_000_000i128);
    }

    for action in input.actions.into_iter().take(40) {
        match action {
            Action::CreateLock {
                user_idx,
                amount,
                duration_ledgers,
            } => {
                let user = &users[user_idx as usize % users.len()];
                let amt = (amount as i128).abs();
                let _ = client.try_create_lock(user, &amt, &duration_ledgers);
            }
            Action::IncreaseAmount {
                user_idx,
                additional_amount,
            } => {
                let user = &users[user_idx as usize % users.len()];
                let amt = (additional_amount as i128).abs();
                let _ = client.try_increase_lock_amount(user, &amt);
            }
            Action::ExtendDuration {
                user_idx,
                additional_duration,
            } => {
                let user = &users[user_idx as usize % users.len()];
                let _ = client.try_extend_lock(user, &additional_duration);
            }
            Action::Withdraw { user_idx } => {
                let user = &users[user_idx as usize % users.len()];
                let _ = client.try_withdraw(user);
            }
            Action::AdvanceLedgers { delta } => {
                let current = env.ledger().sequence();
                env.ledger()
                    .set_sequence_number(current.saturating_add(delta as u32));
            }
        }

        // --- Invariant 1: Locked amounts never go negative ---
        let total_locked = client.get_total_locked();
        assert!(
            total_locked >= 0,
            "Total locked must not be negative: {}",
            total_locked
        );

        for user in &users {
            if let Some(lock) = client.get_lock(user) {
                assert!(
                    lock.amount >= 0,
                    "Lock amount must not be negative: {}",
                    lock.amount
                );

                // --- Invariant 2: Decay never increases voting power ---
                let current_votes = client.get_votes(user);
                assert!(
                    current_votes >= 0,
                    "Current votes must not be negative: {}",
                    current_votes
                );

                let current_seq = env.ledger().sequence();
                if !lock.withdrawn
                    && current_seq >= lock.start_ledger
                    && current_seq < lock.end_ledger
                {
                    // Votes at current ledger cannot exceed initial voting power
                    assert!(
                        current_votes <= lock.initial_voting_power,
                        "Current votes {} exceeds initial power {}",
                        current_votes,
                        lock.initial_voting_power
                    );
                    // Power at any future ledger during the lock cannot exceed current power
                    let future_seq = current_seq.saturating_add(1).min(lock.end_ledger);
                    let future_votes = client.get_past_votes(user, &future_seq);
                    assert!(
                        future_votes <= current_votes,
                        "Decay increased voting power from {} at ledger {} to {} at ledger {}",
                        current_votes,
                        current_seq,
                        future_votes,
                        future_seq
                    );
                }
            }
        }
    }
});

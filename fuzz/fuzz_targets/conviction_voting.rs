//! Fuzz random create / stake / withdraw / checkpoint / cancel sequences
//! against the conviction-voting track.
//!
//! Conviction here is the multiplier a stake earns by staying put: it accrues
//! towards a steady state the longer a stake is held and decays towards the
//! new steady state when stake is withdrawn. The target checks that:
//! - while a proposal's stake is unchanged, conviction moves monotonically
//!   towards its steady state as ledgers pass, so a longer stake duration never
//!   yields less conviction (allowing for integer rounding);
//! - the dynamic execution threshold never decreases as the requested amount
//!   grows;
//! - each staker backs at most one proposal, stakes are positive, and
//!   conviction is never negative;
//! - executed and cancelled proposals are terminal and stop accruing;
//! - a rejected call is a typed `ConvictionVotingError` and leaves the
//!   proposal untouched.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger as _},
    Address, Bytes, Env, InvokeError, Symbol,
};
use sorogov_conviction_voting::{
    ConvictionVotingContract, ConvictionVotingContractClient, ConvictionVotingError,
};

const USERS: usize = 4;
const BPS: i128 = 10_000;
/// Rounding slack allowed when comparing consecutive conviction values.
const ROUNDING: i128 = 2;

#[contracttype]
enum MockKey {
    Votes(Address),
    Supply,
}

#[contract]
pub struct MockVotes;

#[contractimpl]
impl MockVotes {
    pub fn set_votes(env: Env, account: Address, votes: i128) {
        env.storage().instance().set(&MockKey::Votes(account), &votes);
    }

    pub fn set_supply(env: Env, supply: i128) {
        env.storage().instance().set(&MockKey::Supply, &supply);
    }

    pub fn get_votes(env: Env, account: Address) -> i128 {
        env.storage()
            .instance()
            .get(&MockKey::Votes(account))
            .unwrap_or(0)
    }

    pub fn get_past_total_supply(env: Env, _ledger: u32) -> i128 {
        env.storage().instance().get(&MockKey::Supply).unwrap_or(0)
    }
}

/// Target of executed proposals, so execution itself never fails.
#[contract]
pub struct MockTarget;

#[contractimpl]
impl MockTarget {
    pub fn ping(_env: Env) {}
}

#[derive(Arbitrary, Debug, Clone, Copy)]
struct Amount {
    kind: u8,
    raw: u64,
}

impl Amount {
    fn value(self) -> i128 {
        let raw = self.raw as i128;
        match self.kind % 4 {
            0 => raw % 3 - 1,
            1 => raw % 100_000,
            2 => raw,
            _ => raw << (self.raw % 57),
        }
    }
}

#[derive(Arbitrary, Debug)]
enum Op {
    CreateProposal { who: u8, requested: Amount },
    Stake { who: u8, proposal: u8, amount: Amount },
    WithdrawStake { who: u8 },
    Checkpoint { proposal: u8 },
    Cancel { who: u8, proposal: u8 },
    SetVotes { who: u8, votes: Amount },
    SetSupply { supply: Amount },
    Advance { ledgers: u16 },
    ThresholdPair { a: Amount, b: Amount },
}

#[derive(Arbitrary, Debug)]
struct Input {
    decay_bps: u16,
    max_ratio_bps: u16,
    min_threshold: u32,
    weight_bps: u16,
    ops: Vec<Op>,
}

fn typed<T: core::fmt::Debug, C: core::fmt::Debug>(
    what: &str,
    result: Result<Result<T, C>, Result<soroban_sdk::Error, InvokeError>>,
) -> Option<T> {
    match result {
        Ok(Ok(value)) => Some(value),
        Ok(Err(e)) => panic!("{what}: return value failed to convert: {e:?}"),
        Err(Ok(e)) => match ConvictionVotingError::try_from(e) {
            Ok(_) => None,
            Err(_) => panic!("{what}: failed without a typed ConvictionVotingError: {e:?}"),
        },
        Err(Err(e)) => panic!("{what}: panicked without a typed error: {e:?}"),
    }
}

fuzz_target!(|input: Input| {
    let env = Env::default();
    env.mock_all_auths();
    // Let entries outlive the largest ledger jump the target makes.
    env.ledger().with_mut(|l| {
        l.min_persistent_entry_ttl = 1_000_000;
        l.min_temp_entry_ttl = 1_000_000;
        l.max_entry_ttl = 1_000_000;
    });

    let admin = Address::generate(&env);
    let votes_id = env.register(MockVotes, ());
    let target_id = env.register(MockTarget, ());
    let votes = MockVotesClient::new(&env, &votes_id);
    let id = env.register(ConvictionVotingContract, ());
    let client = ConvictionVotingContractClient::new(&env, &id);

    let decay_bps = 1 + u32::from(input.decay_bps) % 9_999;
    let max_ratio_bps = 1 + u32::from(input.max_ratio_bps) % 10_000;
    let weight_bps = 1 + u32::from(input.weight_bps) % 10_000;
    client.initialize(
        &admin,
        &votes_id,
        &decay_bps,
        &max_ratio_bps,
        &(i128::from(input.min_threshold) + 1),
        &weight_bps,
    );

    let users: Vec<Address> = (0..USERS).map(|_| Address::generate(&env)).collect();
    let mut proposals: u64 = 0;
    // Last (ledger, total stake, conviction) seen per proposal, used to check
    // monotonic accrual between observations with unchanged stake.
    let mut last_seen: Vec<Option<(u32, i128, i128)>> = Vec::new();

    let stakes_of = |proposal: u64| -> Vec<(Address, i128)> {
        client
            .get_stakes(&proposal, &0, &1_000)
            .iter()
            .map(|s| (s.staker, s.amount))
            .collect()
    };

    for op in input.ops.iter().take(64) {
        let pick = |raw: u8| &users[raw as usize % USERS];
        let proposal_ids = proposals + 1;

        match op {
            Op::CreateProposal { who, requested } => {
                let result = client.try_create_proposal(
                    pick(*who),
                    &target_id,
                    &Symbol::new(&env, "ping"),
                    &Bytes::new(&env),
                    &requested.value(),
                );
                if let Some(new_id) = typed("create_proposal", result) {
                    proposals += 1;
                    last_seen.push(None);
                    assert_eq!(new_id, proposals, "proposal ids must be sequential");
                }
            }
            Op::Stake { who, proposal, amount } => {
                let pid = u64::from(*proposal) % proposal_ids + 1;
                let before = (pid <= proposals).then(|| client.get_proposal(&pid));
                let result = client.try_stake(pick(*who), &pid, &amount.value());
                if typed("stake", result).is_none() {
                    if let Some(before) = before {
                        assert_eq!(client.get_proposal(&pid), before, "failed stake changed the proposal");
                    }
                }
            }
            Op::WithdrawStake { who } => {
                let _ = typed("withdraw_stake", client.try_withdraw_stake(pick(*who)));
            }
            Op::Checkpoint { proposal } => {
                let pid = u64::from(*proposal) % proposal_ids + 1;
                let before = (pid <= proposals).then(|| client.get_proposal(&pid));
                let result = client.try_checkpoint_conviction(&pid);
                match (typed("checkpoint_conviction", result), before) {
                    (Some(conviction), Some(before)) => {
                        let after = client.get_proposal(&pid);
                        assert_eq!(after.conviction, conviction);
                        if before.executed || before.cancelled {
                            assert_eq!(after.conviction, before.conviction, "terminal proposal kept accruing");
                            assert_eq!(after.executed, before.executed);
                        } else {
                            // Executed proposals must have met the threshold at execution.
                            if after.executed {
                                let threshold = client.get_required_threshold(&after.requested_amount);
                                assert!(after.conviction >= threshold, "executed below threshold");
                            }
                            let total: i128 = stakes_of(pid).iter().map(|(_, a)| *a).sum();
                            let now = env.ledger().sequence();
                            if let Some((seen_at, seen_total, seen_conviction)) = last_seen[pid as usize - 1] {
                                let steady = total.saturating_mul(i128::from(weight_bps))
                                    / (BPS - i128::from(decay_bps)).max(1);
                                if seen_total == total && now > seen_at {
                                    if seen_conviction <= steady {
                                        assert!(
                                            after.conviction + ROUNDING >= seen_conviction,
                                            "conviction fell while accruing towards steady state"
                                        );
                                        assert!(
                                            after.conviction <= steady + ROUNDING,
                                            "conviction overshot steady state"
                                        );
                                    } else {
                                        assert!(
                                            after.conviction <= seen_conviction,
                                            "conviction grew while above steady state"
                                        );
                                    }
                                }
                            }
                            last_seen[pid as usize - 1] = Some((now, total, after.conviction));
                        }
                    }
                    (None, Some(before)) => {
                        assert_eq!(client.get_proposal(&pid), before, "failed checkpoint changed the proposal");
                    }
                    _ => {}
                }
            }
            Op::Cancel { who, proposal } => {
                let pid = u64::from(*proposal) % proposal_ids + 1;
                if typed("cancel_proposal", client.try_cancel_proposal(pick(*who), &pid)).is_some() {
                    assert!(client.get_proposal(&pid).cancelled);
                }
            }
            Op::SetVotes { who, votes: amount } => votes.set_votes(pick(*who), &amount.value()),
            Op::SetSupply { supply } => votes.set_supply(&supply.value()),
            Op::Advance { ledgers } => {
                env.ledger()
                    .with_mut(|l| l.sequence_number += u32::from(*ledgers) % 1_024);
            }
            Op::ThresholdPair { a, b } => {
                let (lo, hi) = (a.value().max(0), b.value().max(0));
                let (lo, hi) = (lo.min(hi), lo.max(hi));
                let t_lo = typed("get_required_threshold", client.try_get_required_threshold(&lo));
                let t_hi = typed("get_required_threshold", client.try_get_required_threshold(&hi));
                if let (Some(t_lo), Some(t_hi)) = (t_lo, t_hi) {
                    assert!(t_lo <= t_hi, "threshold fell as requested amount grew: {lo} -> {t_lo}, {hi} -> {t_hi}");
                }
            }
        }

        // Cross-proposal invariants.
        let mut staking: Vec<Address> = Vec::new();
        for pid in 1..=proposals {
            let proposal = client.get_proposal(&pid);
            assert!(proposal.conviction >= 0, "negative conviction");
            assert!(!(proposal.executed && proposal.cancelled), "executed and cancelled");
            for (staker, amount) in stakes_of(pid) {
                assert!(amount > 0, "non-positive stake");
                assert!(!staking.contains(&staker), "staker backs more than one proposal");
                staking.push(staker);
            }
        }
    }
});

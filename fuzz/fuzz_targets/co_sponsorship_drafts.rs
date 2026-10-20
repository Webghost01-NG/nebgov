//! Fuzz random create / co-sponsor / withdraw / finalize / cancel sequences
//! against the co-sponsorship draft registry.
//!
//! After every call the target checks, for every draft ever created, that:
//! - the sum of the per-sponsor powers equals the draft's `total_power`;
//! - `co_sponsors` and `co_sponsor_power` stay the same length, hold no
//!   duplicates, never exceed `max_co_sponsors`, and agree with
//!   `get_co_sponsor_power`;
//! - a draft is never both finalized and cancelled;
//! - a rejected call is a typed `CoSponsorshipError` and leaves the draft
//!   exactly as it was.
//!
//! Voting power and the proposal threshold are served by mock contracts so the
//! target can change them between calls, as real token balances would.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Bytes, BytesN, Env, InvokeError, String, Symbol, Vec as SorobanVec,
};
use sorogov_co_sponsorship::{
    error::CoSponsorshipError, CoSponsorshipContract, CoSponsorshipContractClient, ProposalDraft,
};

const USERS: usize = 4;
/// Keeps `sum(power)` for the maximum sponsor count well inside i128.
const MAX_POWER: i128 = 1 << 120;

/// Mocks live in their own module so `Vec` can name `soroban_sdk::Vec`, which
/// the contract macros require, while the target itself uses `std::vec::Vec`.
mod mocks {
    use soroban_sdk::{contract, contractimpl, contracttype, Address, Bytes, BytesN, Env, String, Symbol, Vec};

    #[contracttype]
    pub enum MockKey {
        Power(Address),
        Threshold,
        Proposals,
    }

    #[contract]
    pub struct MockVotes;

    #[contractimpl]
    impl MockVotes {
        pub fn set_power(env: Env, account: Address, power: i128) {
            env.storage().instance().set(&MockKey::Power(account), &power);
        }

        pub fn get_votes(env: Env, account: Address) -> i128 {
            env.storage()
                .instance()
                .get(&MockKey::Power(account))
                .unwrap_or(0)
        }
    }

    #[contract]
    pub struct MockGovernor;

    #[contractimpl]
    impl MockGovernor {
        pub fn set_threshold(env: Env, threshold: i128) {
            env.storage().instance().set(&MockKey::Threshold, &threshold);
        }

        pub fn proposal_threshold(env: Env) -> i128 {
            env.storage().instance().get(&MockKey::Threshold).unwrap_or(0)
        }

        pub fn get_effective_threshold(env: Env, _proposer: Address) -> i128 {
            Self::proposal_threshold(env)
        }

        #[allow(clippy::too_many_arguments)]
        pub fn propose_via_registry(
            env: Env,
            _registry: Address,
            _proposer: Address,
            _description: String,
            _description_hash: BytesN<32>,
            _metadata_uri: String,
            _targets: Vec<Address>,
            _fn_names: Vec<Symbol>,
            _calldatas: Vec<Bytes>,
        ) -> u64 {
            let count: u64 = env.storage().instance().get(&MockKey::Proposals).unwrap_or(0) + 1;
            env.storage().instance().set(&MockKey::Proposals, &count);
            count
        }
    }

}
use mocks::{MockGovernor, MockGovernorClient, MockVotes, MockVotesClient};

#[derive(Arbitrary, Debug, Clone, Copy)]
struct Power {
    kind: u8,
    raw: u64,
}

impl Power {
    fn value(self) -> i128 {
        let raw = self.raw as i128;
        let value = match self.kind % 4 {
            0 => raw % 3 - 1,
            1 => raw % 1_000,
            2 => raw,
            _ => raw << (self.raw % 57),
        };
        value.min(MAX_POWER)
    }
}

#[derive(Arbitrary, Debug)]
enum Op {
    CreateDraft {
        who: u8,
        targets: u8,
        mismatched_lengths: bool,
        oversized_calldata: bool,
    },
    CoSponsor { who: u8, draft: u8 },
    Withdraw { who: u8, draft: u8 },
    Finalize { who: u8, draft: u8 },
    Cancel { who: u8, draft: u8 },
    SetPower { who: u8, power: Power },
    SetThreshold { threshold: Power },
    Advance { ledgers: u8 },
}

#[derive(Arbitrary, Debug)]
struct Input {
    draft_expiry_ledgers: u8,
    max_co_sponsors: u8,
    ops: Vec<Op>,
}

/// Snapshot of the mutable draft fields, for "failed call changed nothing".
#[derive(Debug, PartialEq)]
struct DraftState {
    sponsors: Vec<Address>,
    powers: Vec<i128>,
    total_power: i128,
    finalized: bool,
    cancelled: bool,
}

fn state(draft: &ProposalDraft) -> DraftState {
    DraftState {
        sponsors: draft.co_sponsors.iter().collect(),
        powers: draft.co_sponsor_power.iter().collect(),
        total_power: draft.total_power,
        finalized: draft.finalized,
        cancelled: draft.cancelled,
    }
}

/// Returns the call's value, or `None` if it failed with a typed contract
/// error. Anything else is a bug.
fn typed<T: core::fmt::Debug, C: core::fmt::Debug>(
    what: &str,
    result: Result<Result<T, C>, Result<soroban_sdk::Error, InvokeError>>,
) -> Option<T> {
    match result {
        Ok(Ok(value)) => Some(value),
        Ok(Err(e)) => panic!("{what}: return value failed to convert: {e:?}"),
        Err(Ok(e)) => match CoSponsorshipError::try_from(e) {
            Ok(_) => None,
            Err(_) => panic!("{what}: failed without a typed CoSponsorshipError: {e:?}"),
        },
        Err(Err(e)) => panic!("{what}: panicked without a typed error: {e:?}"),
    }
}

fuzz_target!(|input: Input| {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let votes_id = env.register(MockVotes, ());
    let governor_id = env.register(MockGovernor, ());
    let votes = MockVotesClient::new(&env, &votes_id);
    let governor = MockGovernorClient::new(&env, &governor_id);

    let registry_id = env.register(CoSponsorshipContract, ());
    let client = CoSponsorshipContractClient::new(&env, &registry_id);
    let max_co_sponsors = u32::from(input.max_co_sponsors % 8) + 1;
    client.initialize(
        &admin,
        &governor_id,
        &votes_id,
        &(u32::from(input.draft_expiry_ledgers) + 1),
        &max_co_sponsors,
    );

    let users: Vec<Address> = (0..USERS).map(|_| Address::generate(&env)).collect();
    let mut draft_count: u64 = 0;

    for op in input.ops.iter().take(64) {
        let pick = |raw: u8| &users[raw as usize % USERS];

        match op {
            Op::CreateDraft {
                who,
                targets,
                mismatched_lengths,
                oversized_calldata,
            } => {
                let n = u32::from(*targets % 4);
                let mut target_list = SorobanVec::new(&env);
                let mut fn_names = SorobanVec::new(&env);
                let mut calldatas = SorobanVec::new(&env);
                for _ in 0..n {
                    target_list.push_back(Address::generate(&env));
                    fn_names.push_back(Symbol::new(&env, "run"));
                    let calldata = if *oversized_calldata {
                        Bytes::from_slice(&env, &[0u8; 10_001])
                    } else {
                        Bytes::new(&env)
                    };
                    calldatas.push_back(calldata);
                }
                if *mismatched_lengths {
                    fn_names.push_back(Symbol::new(&env, "extra"));
                }
                let result = client.try_create_draft(
                    pick(*who),
                    &String::from_str(&env, "fuzz draft"),
                    &BytesN::from_array(&env, &[7u8; 32]),
                    &String::from_str(&env, "ipfs://fuzz"),
                    &target_list,
                    &fn_names,
                    &calldatas,
                );
                if let Some(id) = typed("create_draft", result) {
                    draft_count += 1;
                    assert_eq!(id, draft_count, "draft ids must be sequential");
                }
            }
            Op::CoSponsor { who, draft } | Op::Withdraw { who, draft } | Op::Finalize { who, draft } | Op::Cancel { who, draft } => {
                // Ids start at 1; reach one past the end to hit DraftNotFound.
                let id = u64::from(*draft) % (draft_count + 1) + 1;
                let before = (id <= draft_count).then(|| state(&client.get_draft(&id)));
                let sponsor = pick(*who);
                let succeeded = match op {
                    Op::CoSponsor { .. } => {
                        typed("co_sponsor", client.try_co_sponsor(sponsor, &id)).is_some()
                    }
                    Op::Withdraw { .. } => {
                        typed("withdraw", client.try_withdraw_co_sponsorship(sponsor, &id)).is_some()
                    }
                    Op::Finalize { .. } => {
                        let proposal = typed("finalize", client.try_finalize_draft(sponsor, &id));
                        if let Some(proposal_id) = proposal {
                            assert!(proposal_id > 0, "finalize returned no proposal id");
                            assert!(client.get_draft(&id).finalized);
                        }
                        proposal.is_some()
                    }
                    _ => typed("cancel", client.try_cancel_draft(sponsor, &id)).is_some(),
                };
                if !succeeded {
                    if let Some(before) = before {
                        assert_eq!(state(&client.get_draft(&id)), before, "failed call changed the draft");
                    }
                }
            }
            Op::SetPower { who, power } => votes.set_power(pick(*who), &power.value()),
            Op::SetThreshold { threshold } => governor.set_threshold(&threshold.value().max(0)),
            Op::Advance { ledgers } => {
                env.ledger().with_mut(|l| l.sequence_number += u32::from(*ledgers % 8));
            }
        }

        for id in 1..=draft_count {
            let draft = client.get_draft(&id);
            let sponsors: Vec<Address> = draft.co_sponsors.iter().collect();
            let powers: Vec<i128> = draft.co_sponsor_power.iter().collect();

            assert_eq!(sponsors.len(), powers.len(), "sponsor / power length mismatch");
            assert!(sponsors.len() as u32 <= max_co_sponsors, "exceeded max co-sponsors");
            assert!(!(draft.finalized && draft.cancelled), "finalized and cancelled");
            assert_eq!(powers.iter().sum::<i128>(), draft.total_power, "power sum != draft total");

            for (i, sponsor) in sponsors.iter().enumerate() {
                assert!(!sponsors[..i].contains(sponsor), "duplicate co-sponsor");
                assert_eq!(client.get_co_sponsor_power(&id, sponsor), powers[i], "per-sponsor power mismatch");
            }
            for user in users.iter().filter(|u| !sponsors.contains(u)) {
                assert_eq!(client.get_co_sponsor_power(&id, user), 0, "non-sponsor has power");
            }
        }
    }
});

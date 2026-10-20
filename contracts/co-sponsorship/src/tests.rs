use crate::{CoSponsorshipContract, CoSponsorshipContractClient};
use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger as _},
    Address, Bytes, BytesN, Env, IntoVal, String, Symbol, Vec,
};

/// Mock votes contract whose voting power is configurable per-address via
/// `set_power`, so tests can exercise partial/aggregate threshold
/// accumulation across multiple distinct co-sponsors.
#[contract]
pub struct MockVotesContract;

#[contracttype]
enum MockVotesKey {
    Power(Address),
}

#[contractimpl]
impl MockVotesContract {
    pub fn set_power(env: Env, account: Address, power: i128) {
        env.storage()
            .instance()
            .set(&MockVotesKey::Power(account), &power);
    }

    pub fn get_votes(env: Env, account: Address) -> i128 {
        env.storage()
            .instance()
            .get(&MockVotesKey::Power(account))
            .unwrap_or(0)
    }
}

/// Mock governor stub implementing just enough of the real governor's
/// interface (`proposal_threshold`, `propose_via_registry`) for
/// `finalize_draft` to be exercised in isolation.
#[contract]
pub struct MockGovernorContract;

#[contracttype]
enum MockGovKey {
    Threshold,
    Count,
}

#[contractimpl]
impl MockGovernorContract {
    pub fn set_threshold(env: Env, threshold: i128) {
        env.storage()
            .instance()
            .set(&MockGovKey::Threshold, &threshold);
    }

    pub fn proposal_threshold(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&MockGovKey::Threshold)
            .unwrap_or(0)
    }

    pub fn get_effective_threshold(env: Env, _proposer: Address) -> i128 {
        env.storage()
            .instance()
            .get(&MockGovKey::Threshold)
            .unwrap_or(0)
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
        let count: u64 = env
            .storage()
            .instance()
            .get(&MockGovKey::Count)
            .unwrap_or(0);
        let id = count + 1;
        env.storage().instance().set(&MockGovKey::Count, &id);
        id
    }
}

fn setup(
    threshold: i128,
) -> (
    Env,
    CoSponsorshipContractClient<'static>,
    MockVotesContractClient<'static>,
    Address, // admin
) {
    setup_with_expiry(threshold, 7_200)
}

fn setup_with_expiry(
    threshold: i128,
    draft_expiry_ledgers: u32,
) -> (
    Env,
    CoSponsorshipContractClient<'static>,
    MockVotesContractClient<'static>,
    Address, // admin
) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let votes_id = env.register(MockVotesContract, ());
    let votes_client = MockVotesContractClient::new(&env, &votes_id);
    let gov_id = env.register(MockGovernorContract, ());
    let gov_client = MockGovernorContractClient::new(&env, &gov_id);
    gov_client.set_threshold(&threshold);

    let contract_id = env.register(CoSponsorshipContract, ());
    let client = CoSponsorshipContractClient::new(&env, &contract_id);
    client.initialize(&admin, &gov_id, &votes_id, &draft_expiry_ledgers, &20u32);

    (env, client, votes_client, admin)
}

fn dummy_action(env: &Env) -> (Vec<Address>, Vec<Symbol>, Vec<Bytes>) {
    (
        soroban_sdk::vec![env, Address::generate(env)],
        soroban_sdk::vec![env, Symbol::new(env, "test")],
        soroban_sdk::vec![env, Bytes::new(env)],
    )
}

fn create_draft(env: &Env, client: &CoSponsorshipContractClient, creator: &Address) -> u64 {
    let (targets, fn_names, calldatas) = dummy_action(env);
    client.create_draft(
        creator,
        &String::from_str(env, "test draft"),
        &env.crypto().sha256(&Bytes::new(env)).into(),
        &String::from_str(env, "https://example.com/draft"),
        &targets,
        &fn_names,
        &calldatas,
    )
}

#[test]
fn test_create_draft_succeeds_below_threshold() {
    let (env, client, votes, _admin) = setup(1_000);
    let creator = Address::generate(&env);
    votes.set_power(&creator, &100);

    let draft_id = create_draft(&env, &client, &creator);
    assert_eq!(draft_id, 1);

    let draft = client.get_draft(&draft_id);
    assert_eq!(draft.creator, creator);
    assert_eq!(draft.total_power, 0);
    assert!(!draft.finalized);
    assert!(!draft.cancelled);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_create_draft_rejects_mismatched_vector_lengths() {
    let (env, client, _votes, _admin) = setup(1_000);
    let creator = Address::generate(&env);
    let targets = soroban_sdk::vec![&env, Address::generate(&env)];
    let fn_names = soroban_sdk::vec![&env, Symbol::new(&env, "test"), Symbol::new(&env, "other")];
    let calldatas = soroban_sdk::vec![&env, Bytes::new(&env)];

    client.create_draft(
        &creator,
        &String::from_str(&env, "test draft"),
        &env.crypto().sha256(&Bytes::new(&env)).into(),
        &String::from_str(&env, "https://example.com/draft"),
        &targets,
        &fn_names,
        &calldatas,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #12)")]
fn test_create_draft_rejects_empty_targets() {
    let (env, client, _votes, _admin) = setup(1_000);
    let creator = Address::generate(&env);
    let targets = soroban_sdk::vec![&env];
    let fn_names = soroban_sdk::vec![&env];
    let calldatas = soroban_sdk::vec![&env];

    client.create_draft(
        &creator,
        &String::from_str(&env, "test draft"),
        &env.crypto().sha256(&Bytes::new(&env)).into(),
        &String::from_str(&env, "https://example.com/draft"),
        &targets,
        &fn_names,
        &calldatas,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #13)")]
fn test_create_draft_rejects_oversized_calldata() {
    let (env, client, _votes, _admin) = setup(1_000);
    let creator = Address::generate(&env);
    let targets = soroban_sdk::vec![&env, Address::generate(&env)];
    let fn_names = soroban_sdk::vec![&env, Symbol::new(&env, "test")];

    // Create calldata larger than MAX_CALLDATA_SIZE (10_000 bytes)
    const LARGE_DATA: &[u8; 10_001] = &[0x42u8; 10_001];
    let calldata = Bytes::from_array(&env, LARGE_DATA);
    let calldatas = soroban_sdk::vec![&env, calldata];

    client.create_draft(
        &creator,
        &String::from_str(&env, "test draft"),
        &env.crypto().sha256(&Bytes::new(&env)).into(),
        &String::from_str(&env, "https://example.com/draft"),
        &targets,
        &fn_names,
        &calldatas,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #14)")]
fn test_create_draft_rejects_too_many_calldata_entries() {
    let (env, client, _votes, _admin) = setup(1_000);
    let creator = Address::generate(&env);
    let mut targets = soroban_sdk::vec![&env];
    let mut fn_names = soroban_sdk::vec![&env];
    let mut calldatas = soroban_sdk::vec![&env];

    // Create 11 entries, exceeding MAX_CALLDATA_COUNT (10)
    for _ in 0..11 {
        targets.push_back(Address::generate(&env));
        fn_names.push_back(Symbol::new(&env, "test"));
        calldatas.push_back(Bytes::new(&env));
    }

    client.create_draft(
        &creator,
        &String::from_str(&env, "test draft"),
        &env.crypto().sha256(&Bytes::new(&env)).into(),
        &String::from_str(&env, "https://example.com/draft"),
        &targets,
        &fn_names,
        &calldatas,
    );
}

#[test]
fn test_co_sponsor_accumulates_power() {
    let (env, client, votes, _admin) = setup(1_000);
    let creator = Address::generate(&env);
    let sponsor_a = Address::generate(&env);
    let sponsor_b = Address::generate(&env);
    votes.set_power(&sponsor_a, &400);
    votes.set_power(&sponsor_b, &300);

    let draft_id = create_draft(&env, &client, &creator);

    client.co_sponsor(&sponsor_a, &draft_id);
    assert_eq!(client.get_draft(&draft_id).total_power, 400);

    client.co_sponsor(&sponsor_b, &draft_id);
    let draft = client.get_draft(&draft_id);
    assert_eq!(draft.total_power, 700);
    assert_eq!(draft.co_sponsors.len(), 2);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_co_sponsor_same_address_twice_reverts() {
    let (env, client, votes, _admin) = setup(1_000);
    let creator = Address::generate(&env);
    let sponsor = Address::generate(&env);
    votes.set_power(&sponsor, &400);

    let draft_id = create_draft(&env, &client, &creator);
    client.co_sponsor(&sponsor, &draft_id);
    client.co_sponsor(&sponsor, &draft_id);
}

#[test]
fn test_withdraw_co_sponsorship_reduces_total_power() {
    let (env, client, votes, _admin) = setup(1_000);
    let creator = Address::generate(&env);
    let sponsor = Address::generate(&env);
    votes.set_power(&sponsor, &400);

    let draft_id = create_draft(&env, &client, &creator);
    client.co_sponsor(&sponsor, &draft_id);
    assert_eq!(client.get_draft(&draft_id).total_power, 400);

    client.withdraw_co_sponsorship(&sponsor, &draft_id);
    let draft = client.get_draft(&draft_id);
    assert_eq!(draft.total_power, 0);
    assert_eq!(draft.co_sponsors.len(), 0);
    assert_eq!(client.get_co_sponsor_power(&draft_id, &sponsor), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_finalize_draft_below_threshold_reverts() {
    let (env, client, votes, _admin) = setup(1_000);
    let creator = Address::generate(&env);
    let sponsor = Address::generate(&env);
    votes.set_power(&sponsor, &400);

    let draft_id = create_draft(&env, &client, &creator);
    client.co_sponsor(&sponsor, &draft_id);
    client.finalize_draft(&creator, &draft_id);
}

#[test]
fn test_finalize_draft_above_threshold_creates_proposal() {
    let (env, client, votes, _admin) = setup(1_000);
    let creator = Address::generate(&env);
    let sponsor_a = Address::generate(&env);
    let sponsor_b = Address::generate(&env);
    votes.set_power(&sponsor_a, &600);
    votes.set_power(&sponsor_b, &500);

    let draft_id = create_draft(&env, &client, &creator);
    client.co_sponsor(&sponsor_a, &draft_id);
    client.co_sponsor(&sponsor_b, &draft_id);
    assert!(client.draft_threshold_met(&draft_id));

    let proposal_id = client.finalize_draft(&creator, &draft_id);
    assert_eq!(proposal_id, 1);

    let draft = client.get_draft(&draft_id);
    assert!(draft.finalized);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_draft_expires_after_expiry_ledger() {
    let (env, client, _votes, _admin) = setup_with_expiry(1_000, 5);
    let creator = Address::generate(&env);
    let draft_id = create_draft(&env, &client, &creator);

    env.ledger()
        .set_sequence_number(env.ledger().sequence() + 50);

    client.finalize_draft(&creator, &draft_id);
}

#[test]
fn test_cancel_draft_by_creator() {
    let (env, client, _votes, _admin) = setup(1_000);
    let creator = Address::generate(&env);
    let draft_id = create_draft(&env, &client, &creator);

    client.cancel_draft(&creator, &draft_id);
    assert!(client.get_draft(&draft_id).cancelled);
}

#[test]
fn test_cancel_draft_by_guardian() {
    // This contract calls the privileged non-creator canceller "admin"
    // rather than "guardian" (it has no separate guardian concept), but the
    // behavior mirrors the governor's guardian-can-veto pattern.
    let (env, client, _votes, admin) = setup(1_000);
    let creator = Address::generate(&env);
    let draft_id = create_draft(&env, &client, &creator);

    client.cancel_draft(&admin, &draft_id);
    assert!(client.get_draft(&draft_id).cancelled);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_cancel_draft_rejects_unrelated_caller() {
    let (env, client, _votes, _admin) = setup(1_000);
    let creator = Address::generate(&env);
    let unrelated = Address::generate(&env);
    let draft_id = create_draft(&env, &client, &creator);

    client.cancel_draft(&unrelated, &draft_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_co_sponsor_limit_reached_reverts() {
    let (env, _client, _votes, admin) = setup(1_000);
    // Re-initialize with max_co_sponsors=1 by deploying a fresh instance
    // configured that way, mirroring `setup` but with a tighter cap.
    let votes_id = env.register(MockVotesContract, ());
    let votes = MockVotesContractClient::new(&env, &votes_id);
    let gov_id = env.register(MockGovernorContract, ());
    MockGovernorContractClient::new(&env, &gov_id).set_threshold(&1_000);
    let contract_id = env.register(CoSponsorshipContract, ());
    let client = CoSponsorshipContractClient::new(&env, &contract_id);
    client.initialize(&admin, &gov_id, &votes_id, &7_200u32, &1u32);

    let creator = Address::generate(&env);
    let sponsor_a = Address::generate(&env);
    let sponsor_b = Address::generate(&env);
    votes.set_power(&sponsor_a, &100);
    votes.set_power(&sponsor_b, &100);

    let draft_id = create_draft(&env, &client, &creator);
    client.co_sponsor(&sponsor_a, &draft_id);
    client.co_sponsor(&sponsor_b, &draft_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_co_sponsor_on_finalized_draft_reverts() {
    let (env, client, votes, _admin) = setup(1_000);
    let creator = Address::generate(&env);
    let sponsor_a = Address::generate(&env);
    let sponsor_b = Address::generate(&env);
    votes.set_power(&sponsor_a, &1_000);
    votes.set_power(&sponsor_b, &100);

    let draft_id = create_draft(&env, &client, &creator);
    client.co_sponsor(&sponsor_a, &draft_id);
    client.finalize_draft(&creator, &draft_id);

    client.co_sponsor(&sponsor_b, &draft_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_co_sponsor_on_expired_draft_reverts() {
    let (env, client, votes, _admin) = setup_with_expiry(1_000, 5);
    let creator = Address::generate(&env);
    let sponsor = Address::generate(&env);
    votes.set_power(&sponsor, &400);

    let draft_id = create_draft(&env, &client, &creator);
    env.ledger()
        .set_sequence_number(env.ledger().sequence() + 50);

    client.co_sponsor(&sponsor, &draft_id);
}

#[test]
fn test_get_active_drafts_pagination() {
    let (env, client, _votes, _admin) = setup(1_000);
    let creator = Address::generate(&env);

    for _ in 0..5 {
        create_draft(&env, &client, &creator);
    }

    let page1 = client.get_active_drafts(&0, &2);
    assert_eq!(page1.len(), 2);
    assert_eq!(page1.get(0).unwrap().id, 1);
    assert_eq!(page1.get(1).unwrap().id, 2);

    let page2 = client.get_active_drafts(&2, &2);
    assert_eq!(page2.len(), 2);
    assert_eq!(page2.get(0).unwrap().id, 3);
    assert_eq!(page2.get(1).unwrap().id, 4);

    let page3 = client.get_active_drafts(&4, &2);
    assert_eq!(page3.len(), 1);
    assert_eq!(page3.get(0).unwrap().id, 5);

    let page4 = client.get_active_drafts(&10, &2);
    assert_eq!(page4.len(), 0);
}

#[test]
fn test_get_active_drafts_excludes_finalized_and_cancelled() {
    let (env, client, votes, _admin) = setup(1_000);
    let creator = Address::generate(&env);
    let sponsor = Address::generate(&env);
    votes.set_power(&sponsor, &2_000);

    let open_id = create_draft(&env, &client, &creator);
    let finalized_id = create_draft(&env, &client, &creator);
    let cancelled_id = create_draft(&env, &client, &creator);

    client.co_sponsor(&sponsor, &finalized_id);
    client.finalize_draft(&creator, &finalized_id);
    client.cancel_draft(&creator, &cancelled_id);

    let active = client.get_active_drafts(&0, &10);
    assert_eq!(active.len(), 1);
    assert_eq!(active.get(0).unwrap().id, open_id);
    assert_ne!(open_id, finalized_id);
    assert_ne!(open_id, cancelled_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_finalize_draft_rechecks_current_power_not_stale_pledge() {
    let (env, client, votes, _admin) = setup(1_000);
    let creator = Address::generate(&env);
    let sponsor = Address::generate(&env);
    votes.set_power(&sponsor, &2_000);

    let draft_id = create_draft(&env, &client, &creator);
    client.co_sponsor(&sponsor, &draft_id);
    assert_eq!(client.get_draft(&draft_id).total_power, 2_000);

    // Sponsor's power drops below the threshold after pledging (e.g. they
    // transferred tokens away). The cached `total_power` on the draft is
    // now stale/inflated; finalize must re-check live power and reject.
    votes.set_power(&sponsor, &100);

    client.finalize_draft(&creator, &draft_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_finalize_draft_rejects_non_creator() {
    let (env, client, votes, _admin) = setup(1_000);
    let creator = Address::generate(&env);
    let attacker = Address::generate(&env);
    let sponsor = Address::generate(&env);
    votes.set_power(&sponsor, &2_000);

    let draft_id = create_draft(&env, &client, &creator);
    client.co_sponsor(&sponsor, &draft_id);
    client.finalize_draft(&attacker, &draft_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_initialize_rejects_reinitialization() {
    let (env, _client, _votes, admin) = setup(1_000);
    let votes_id = env.register(MockVotesContract, ());
    let gov_id = env.register(MockGovernorContract, ());
    let contract_id = env.register(CoSponsorshipContract, ());
    let client = CoSponsorshipContractClient::new(&env, &contract_id);
    client.initialize(&admin, &gov_id, &votes_id, &7_200u32, &20u32);
    client.initialize(&admin, &gov_id, &votes_id, &7_200u32, &20u32);
}

/// Regression test for #866/#850: a successful `finalize_draft` rechecks each
/// co-sponsor's *current* voting power (not their power at pledge time)
/// and updates both `draft.total_power` and the per-sponsor breakdown to the
/// rechecked values.
#[test]
fn test_finalize_draft_success_with_changed_sponsor_power() {
    let (env, client, votes, _admin) = setup(1_000);
    let creator = Address::generate(&env);
    let sponsor_a = Address::generate(&env);
    let sponsor_b = Address::generate(&env);
    votes.set_power(&sponsor_a, &600);
    votes.set_power(&sponsor_b, &500);

    let draft_id = create_draft(&env, &client, &creator);
    client.co_sponsor(&sponsor_a, &draft_id);
    client.co_sponsor(&sponsor_b, &draft_id);

    let draft = client.get_draft(&draft_id);
    assert_eq!(draft.total_power, 1_100);
    assert_eq!(draft.co_sponsor_power.get(0).unwrap(), 600);
    assert_eq!(draft.co_sponsor_power.get(1).unwrap(), 500);

    // Sponsor A's live power increases (still above threshold even alone)
    votes.set_power(&sponsor_a, &800);

    let proposal_id = client.finalize_draft(&creator, &draft_id);
    assert_eq!(proposal_id, 1);

    let finalized = client.get_draft(&draft_id);
    assert!(finalized.finalized);

    // total_power reflects live recheck (800 + 500 = 1300, not stale 1100)
    assert_eq!(
        finalized.total_power, 1_300,
        "total_power must be rechecked against live voting power"
    );

    // Per-sponsor entries must also be rechecked to match the live values.
    assert_eq!(
        finalized.co_sponsor_power.get(0).unwrap(),
        800,
        "co_sponsor_power[0] should reflect the sponsor's latest live power"
    );
    assert_eq!(
        finalized.co_sponsor_power.get(1).unwrap(),
        500,
        "co_sponsor_power[1] should remain unchanged when its power is unchanged"
    );

    // Same for the per-sponsor storage key.
    assert_eq!(
        client.get_co_sponsor_power(&draft_id, &sponsor_a),
        800,
        "get_co_sponsor_power should return the latest live power"
    );
    assert_eq!(
        client.get_co_sponsor_power(&draft_id, &sponsor_b),
        500,
        "get_co_sponsor_power should return the latest live power"
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #15)")]
fn test_uninitialized_calls_revert() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CoSponsorshipContract, ());
    let client = CoSponsorshipContractClient::new(&env, &contract_id);
    // Directly invoke the helper that checks for initialization
    client.check_not_initialized();
}

#[test]
#[should_panic(expected = "Error(Contract, #16)")]
fn test_index_out_of_bounds_reverts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CoSponsorshipContract, ());
    let client = CoSponsorshipContractClient::new(&env, &contract_id);
    client.check_index_out_of_bounds();
}

#[test]
fn test_admin_setters_succeed() {
    let (env, client, _votes, _admin) = setup(1_000);

    let new_gov = Address::generate(&env);
    let new_votes = Address::generate(&env);

    client.set_governor(&new_gov);
    client.set_votes_token(&new_votes);
    client.set_draft_expiry_ledgers(&10_000u32);
    client.set_max_co_sponsors(&50u32);

    let creator = Address::generate(&env);
    let draft_id = create_draft(&env, &client, &creator);
    let draft = client.get_draft(&draft_id);
    assert_eq!(draft.expiry_ledger, env.ledger().sequence() + 10_000);
}

#[test]
#[should_panic]
fn test_admin_setters_reject_non_admin() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let votes_id = env.register(MockVotesContract, ());
    let gov_id = env.register(MockGovernorContract, ());
    let contract_id = env.register(CoSponsorshipContract, ());
    let client = CoSponsorshipContractClient::new(&env, &contract_id);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "initialize",
            args: (&admin, &gov_id, &votes_id, &7200u32, &20u32).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.initialize(&admin, &gov_id, &votes_id, &7200u32, &20u32);

    let new_gov = Address::generate(&env);
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &non_admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "set_governor",
            args: (&new_gov,).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.set_governor(&new_gov);
}

#[test]
fn test_withdraw_co_sponsorship_succeeds_after_expiry() {
    let (env, client, votes, _admin) = setup_with_expiry(1_000, 5);
    let creator = Address::generate(&env);
    let sponsor = Address::generate(&env);
    votes.set_power(&sponsor, &500);

    let draft_id = create_draft(&env, &client, &creator);
    client.co_sponsor(&sponsor, &draft_id);

    let draft_before = client.get_draft(&draft_id);
    env.ledger()
        .set_sequence_number(draft_before.expiry_ledger + 10);

    client.withdraw_co_sponsorship(&sponsor, &draft_id);

    let draft_after = client.get_draft(&draft_id);
    assert_eq!(draft_after.total_power, 0);
    assert_eq!(client.get_co_sponsor_power(&draft_id, &sponsor), 0);
}

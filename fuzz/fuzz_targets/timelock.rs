//! Fuzz timelock scheduling and dependency validation boundaries.

#![no_main]

use std::vec::Vec;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, Symbol, Vec as SorobanVec};
use sorogov_timelock::{TimelockContract, TimelockContractClient};

#[derive(Arbitrary, Debug)]
struct Input {
    delay: u64,
    execution_window: u64,
    salt: std::vec::Vec<u8>,
    predecessor: std::vec::Vec<u8>,
}

fuzz_target!(|input: Input| {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let governor = Address::generate(&env);
    let target = Address::generate(&env);
    let id = env.register(TimelockContract, ());
    let client = TimelockContractClient::new(&env, &id);

    client.initialize(&admin, &governor, &5, &input.execution_window.max(1));
    let salt = Bytes::from_slice(&env, &input.salt);
    let predecessor = Bytes::from_slice(&env, &input.predecessor);
    let result = client.try_schedule(
        &governor,
        &target,
        &Bytes::new(&env),
        &Symbol::new(&env, "noop"),
        &input.delay,
        &predecessor,
        &salt,
    );
    if let Ok(Ok(op_id)) = result {
        assert!(client.is_pending(&op_id) || client.is_ready(&op_id));
        assert!(!client.is_done(&op_id));
    }

    let mut nodes = SorobanVec::new(&env);
    nodes.push_back(Bytes::from_slice(&env, b"a"));
    nodes.push_back(Bytes::from_slice(&env, b"b"));
    let validation = client.validate_dependency_dag(&nodes);
    assert!(validation.valid || !validation.cycle_path.is_empty());
});

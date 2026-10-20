//! Fuzz random add / remove / swap sequences against the liquidity pool.
//!
//! After every call the target checks that:
//! - a failing call is rejected with a typed `LiquidityError` (never an
//!   untyped panic such as an arithmetic overflow) and leaves the pool
//!   untouched;
//! - reserves and total LP supply are never negative, and an empty pool has
//!   no reserves while a funded pool has both;
//! - the sum of every provider's LP position equals `total_lp_supply`;
//! - the contract's real token balances equal the recorded reserves;
//! - a swap never decreases the constant-product invariant and never pays out
//!   more than the output reserve.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{
    testutils::Address as _,
    token::{StellarAssetClient, TokenClient},
    Address, Env, InvokeError,
};
use sorogov_liquidity::{LiquidityContract, LiquidityContractClient, LiquidityError, Pool};

const POOL_LO: u32 = 1;
const POOL_HI: u32 = 2;
/// Outcome ids a call may name; most land on the real pool, some do not.
const OUTCOMES: [u32; 6] = [1, 2, 1, 2, 0, 3];
const PROVIDERS: usize = 3;
/// Large enough that token transfers never fail for the amounts below.
const MINTED: i128 = i128::MAX / 8;

/// An amount biased towards the interesting regions: invalid, near the
/// minimum liquidity, realistic, and large enough to overflow i128 products.
/// No value exceeds `MINTED`, so token transfers never fail on balance.
#[derive(Arbitrary, Debug, Clone, Copy)]
struct Amount {
    kind: u8,
    raw: u64,
}

impl Amount {
    fn value(self) -> i128 {
        let raw = self.raw as i128;
        match self.kind % 8 {
            0 => raw % 2_000 - 1_000,
            1 => 1_000 + raw % 10_000_000,
            2 => raw,
            3 => raw << (self.raw % 37),
            // Boundaries where i128 products start to overflow.
            4 => 1 << 64,
            5 => 1 << 70,
            6 => 1 << 100,
            _ => MINTED,
        }
    }
}

#[derive(Arbitrary, Debug)]
enum RemoveAmount {
    /// `numerator / 255` of the provider's current position.
    Fraction(u8),
    Exact(Amount),
}

#[derive(Arbitrary, Debug)]
enum Op {
    Add {
        who: u8,
        outcome_a: u8,
        outcome_b: u8,
        amount_a: Amount,
        amount_b: Amount,
        min_lp_out: Amount,
    },
    Remove {
        who: u8,
        outcome_a: u8,
        outcome_b: u8,
        lp: RemoveAmount,
    },
    Swap {
        who: u8,
        outcome_in: u8,
        outcome_out: u8,
        amount_in: Amount,
        min_out: Amount,
    },
    UpdateFee {
        fee_bps: u16,
    },
}

#[derive(Arbitrary, Debug)]
struct Input {
    initial_fee_bps: u16,
    ops: Vec<Op>,
}

/// Returns the call's value, or `None` if it failed with a typed contract
/// error. Anything else (host abort, overflow panic, bad conversion) is a bug.
fn typed<T: core::fmt::Debug, C: core::fmt::Debug>(
    what: &str,
    result: Result<Result<T, C>, Result<soroban_sdk::Error, InvokeError>>,
) -> Option<T> {
    match result {
        Ok(Ok(value)) => Some(value),
        Ok(Err(e)) => panic!("{what}: return value failed to convert: {e:?}"),
        Err(Ok(e)) => match LiquidityError::try_from(e) {
            Ok(_) => None,
            Err(_) => panic!("{what}: failed without a typed LiquidityError: {e:?}"),
        },
        Err(Err(e)) => panic!("{what}: panicked without a typed error: {e:?}"),
    }
}

fn outcome(raw: u8) -> u32 {
    OUTCOMES[raw as usize % OUTCOMES.len()]
}

fuzz_target!(|input: Input| {
    let env = Env::default();
    env.mock_all_auths();

    let governor = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_a = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_b = env.register_stellar_asset_contract_v2(token_admin).address();

    let liquidity_id = env.register(LiquidityContract, ());
    let client = LiquidityContractClient::new(&env, &liquidity_id);
    let balance_a = TokenClient::new(&env, &token_a);
    let balance_b = TokenClient::new(&env, &token_b);

    let users: Vec<Address> = (0..PROVIDERS).map(|_| Address::generate(&env)).collect();
    for user in &users {
        StellarAssetClient::new(&env, &token_a).mint(user, &MINTED);
        StellarAssetClient::new(&env, &token_b).mint(user, &MINTED);
    }

    client.initialize(&governor);
    client.create_pool(&governor, &POOL_LO, &POOL_HI, &token_a, &token_b);
    let fee = u32::from(input.initial_fee_bps) % 1_100;
    if typed(
        "initialize_pool",
        client.try_initialize_pool(&governor, &POOL_LO, &POOL_HI, &fee),
    )
    .is_none()
    {
        return;
    }

    let read_pool = || -> Pool { client.get_pool(&POOL_LO, &POOL_HI) };

    for op in input.ops.iter().take(32) {
        let before = read_pool();

        match op {
            Op::Add {
                who,
                outcome_a,
                outcome_b,
                amount_a,
                amount_b,
                min_lp_out,
            } => {
                let provider = &users[*who as usize % PROVIDERS];
                let result = client.try_add_liquidity(
                    provider,
                    &outcome(*outcome_a),
                    &outcome(*outcome_b),
                    &amount_a.value(),
                    &amount_b.value(),
                    &min_lp_out.value(),
                );
                match typed("add_liquidity", result) {
                    Some((lp_tokens, _)) => assert!(lp_tokens > 0, "minted no LP shares"),
                    None => assert_eq!(read_pool(), before, "failed add changed the pool"),
                }
            }
            Op::Remove {
                who,
                outcome_a,
                outcome_b,
                lp,
            } => {
                let provider = &users[*who as usize % PROVIDERS];
                let (oa, ob) = (outcome(*outcome_a), outcome(*outcome_b));
                let lp_tokens = match lp {
                    RemoveAmount::Fraction(n) => {
                        client.get_lp_position(provider, &oa, &ob) / 255 * i128::from(*n)
                    }
                    RemoveAmount::Exact(amount) => amount.value(),
                };
                let result = client.try_remove_liquidity(provider, &oa, &ob, &lp_tokens);
                match typed("remove_liquidity", result) {
                    Some((out_a, out_b)) => {
                        assert!(out_a >= 0 && out_b >= 0, "negative payout");
                        assert!(
                            out_a.max(out_b) <= before.reserve_a.max(before.reserve_b),
                            "payout exceeds reserves"
                        );
                    }
                    None => assert_eq!(read_pool(), before, "failed remove changed the pool"),
                }
            }
            Op::Swap {
                who,
                outcome_in,
                outcome_out,
                amount_in,
                min_out,
            } => {
                let trader = &users[*who as usize % PROVIDERS];
                let (oi, oo) = (outcome(*outcome_in), outcome(*outcome_out));
                let result =
                    client.try_swap(trader, &oi, &oo, &amount_in.value(), &min_out.value());
                match typed("swap", result) {
                    Some(amount_out) => {
                        let after = read_pool();
                        let (reserve_out_before, reserve_out_after) = if oi < oo {
                            (before.reserve_b, after.reserve_b)
                        } else {
                            (before.reserve_a, after.reserve_a)
                        };
                        assert!(amount_out >= 0, "negative swap output");
                        assert!(amount_out < reserve_out_before, "drained the output reserve");
                        assert_eq!(reserve_out_before - amount_out, reserve_out_after);
                        if let (Some(k_before), Some(k_after)) = (
                            before.reserve_a.checked_mul(before.reserve_b),
                            after.reserve_a.checked_mul(after.reserve_b),
                        ) {
                            assert!(k_after >= k_before, "swap decreased the pool invariant");
                        }
                    }
                    None => assert_eq!(read_pool(), before, "failed swap changed the pool"),
                }
            }
            Op::UpdateFee { fee_bps } => {
                let fee = u32::from(*fee_bps) % 1_100;
                let result = client.try_update_pool_fee(&governor, &POOL_LO, &POOL_HI, &fee);
                if typed("update_pool_fee", result).is_some() {
                    assert_eq!(read_pool().fee_bps, fee);
                }
            }
        }

        let pool = read_pool();
        assert!(pool.reserve_a >= 0 && pool.reserve_b >= 0, "negative reserve");
        assert!(pool.total_lp_supply >= 0, "negative LP supply");
        if pool.total_lp_supply == 0 {
            assert!(pool.reserve_a == 0 && pool.reserve_b == 0, "reserves without LP supply");
        } else {
            assert!(pool.reserve_a > 0 && pool.reserve_b > 0, "LP supply without reserves");
        }

        let position_sum: i128 = users
            .iter()
            .map(|u| client.get_lp_position(u, &POOL_LO, &POOL_HI))
            .sum();
        assert_eq!(position_sum, pool.total_lp_supply, "LP positions != total supply");

        assert_eq!(balance_a.balance(&liquidity_id), pool.reserve_a, "token A balance != reserve");
        assert_eq!(balance_b.balance(&liquidity_id), pool.reserve_b, "token B balance != reserve");
    }
});

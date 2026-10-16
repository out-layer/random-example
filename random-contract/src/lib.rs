mod types;

use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::{
    env, ext_contract, log, near_bindgen, AccountId, NearToken, PanicOnDefault, Promise,
    PromiseError,
};

use types::{CoinSide, RandomResponse};

/// Minimum deposit to cover OutLayer execution cost
/// (base fee + 10B instructions + 60s of execution at the contract's default pricing)
const MIN_DEPOSIT: u128 = 10_000_000_000_000_000_000_000; // 0.01 NEAR

/// Fixed gas for callback
const CALLBACK_GAS: u64 = 5_000_000_000_000; // 5 TGas

/// The random-example module every flip runs: one build, pinned by its
/// sha256, so a new commit to the repository never changes what a flip
/// executes and no flip waits for a compile.
const WASM_URL: &str = "https://wasmhub.testnet.fastfs.io/fastfs.testnet/0bb0668611f0fa9abeabbfaabf48436a46c94aa65ae1a0d106db7212f2d5f88a.wasm";
const WASM_HASH: &str = "0bb0668611f0fa9abeabbfaabf48436a46c94aa65ae1a0d106db7212f2d5f88a";

/// External contract interface for OutLayer
#[ext_contract(ext_outlayer)]
#[allow(dead_code)]
trait OutLayer {
    fn request_execution(
        &mut self,
        source: near_sdk::serde_json::Value,
        resource_limits: Option<near_sdk::serde_json::Value>,
        input_data: Option<String>,
        secrets_ref: Option<near_sdk::serde_json::Value>,
        response_format: Option<String>,
        payer_account_id: Option<AccountId>,
        params: Option<near_sdk::serde_json::Value>,
    );
}

/// External contract interface for self callbacks
#[ext_contract(ext_self)]
#[allow(dead_code)]
trait ExtSelf {
    fn on_random_result(
        &mut self,
        player: AccountId,
        choice: CoinSide,
        #[callback_result] result: Result<Option<RandomResponse>, PromiseError>,
    ) -> String;
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
#[borsh(crate = "near_sdk::borsh")]
pub struct CoinFlipContract {
    /// OutLayer contract this example talks to
    outlayer_contract_id: AccountId,
}

/// The state layout `migrate` reads: a contract that carried no fields at all.
#[derive(BorshDeserialize, BorshSerialize)]
#[borsh(crate = "near_sdk::borsh")]
pub struct FieldlessCoinFlipContract {}

#[near_bindgen]
impl CoinFlipContract {
    /// Initialize the contract
    ///
    /// # Arguments
    /// * `outlayer_contract_id` - OutLayer contract to call. Omit it and the
    ///   network of this account decides: `outlayer.testnet` for a `*.testnet`
    ///   account, `outlayer.near` otherwise.
    #[init]
    pub fn new(outlayer_contract_id: Option<AccountId>) -> Self {
        Self {
            outlayer_contract_id: outlayer_contract_id.unwrap_or_else(default_outlayer_contract_id),
        }
    }

    /// Adopt the current state layout after a code-only deploy
    ///
    /// Deploying over a live account without an init call leaves the old bytes
    /// in `STATE`, which this code cannot read — every method would panic with
    /// "Cannot deserialize the contract state". Call `migrate` in the same
    /// transaction as the deploy and the account keeps its balance, its keys
    /// and its account id.
    ///
    /// # Arguments
    /// * `outlayer_contract_id` - OutLayer contract to call. Omit it and the
    ///   network of this account decides, exactly as in [`Self::new`].
    #[init(ignore_state)]
    pub fn migrate(outlayer_contract_id: Option<AccountId>) -> Self {
        // The guard is spelled out because `#[private]` is not: near-sdk drops
        // that attribute on an init method, and `ignore_state` means anyone
        // reaching this could otherwise aim the example at a contract of their
        // own and collect the deposits `flip_coin` forwards.
        if env::predecessor_account_id() != env::current_account_id() {
            env::panic_str("Method migrate is private");
        }

        // Nothing is carried across: the layout being replaced held no fields.
        // The read still has to happen, and has to succeed, because it is the
        // one thing that tells a migration apart from a first deploy onto an
        // account that has no state at all.
        let _: FieldlessCoinFlipContract = env::state_read()
            .unwrap_or_else(|| env::panic_str("No state to migrate. Deploy with an init call to new instead."));

        Self {
            outlayer_contract_id: outlayer_contract_id.unwrap_or_else(default_outlayer_contract_id),
        }
    }

    /// OutLayer contract this example calls
    pub fn get_outlayer_contract_id(&self) -> AccountId {
        self.outlayer_contract_id.clone()
    }

    /// Flip a coin and guess the result
    ///
    /// # Arguments
    /// * `choice` - Your prediction: Heads or Tails
    ///
    /// # Payment
    /// Attach 0.01 NEAR to pay for OutLayer execution
    ///
    /// # Returns
    /// Promise that will resolve with win/lose message
    #[payable]
    pub fn flip_coin(&mut self, choice: CoinSide) -> Promise {
        let player = env::predecessor_account_id();
        let attached = env::attached_deposit().as_yoctonear();

        assert!(
            attached >= MIN_DEPOSIT,
            "Minimum deposit is 0.01 NEAR to pay for OutLayer execution"
        );

        log!("Player {} chose {:?}. Requesting random number from OutLayer", player, choice);

        // Hardcoded parameters
        let source = near_sdk::serde_json::json!({
            "WasmUrl": {
                "url": WASM_URL,
                "hash": WASM_HASH,
                "build_target": "wasm32-wasip1"
            }
        });

        let resource_limits = near_sdk::serde_json::json!({
            "max_instructions": 10000000000u64,
            "max_memory_mb": 128u32,
            "max_execution_seconds": 60u64
        });

        // Call OutLayer using ext_contract
        // Use with_unused_gas_weight to allocate all remaining gas to request_execution
        // and reserve fixed CALLBACK_GAS for callback
        // Pass player as payer_account_id so refund goes to player, not this contract
        ext_outlayer::ext(self.outlayer_contract_id.clone())
            .with_attached_deposit(NearToken::from_yoctonear(attached))
            .with_unused_gas_weight(1) // All unused gas goes to request_execution
            .request_execution(
                source,
                Some(resource_limits),
                Some("{\"min\":0,\"max\":1}".to_string()),
                None,
                Some("Json".to_string()),
                Some(player.clone()), // Refund to player, not this contract
                None,
            )
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(near_sdk::Gas::from_gas(CALLBACK_GAS))
                    .on_random_result(player, choice),
            )
    }

    /// Callback to handle random number result
    ///
    /// Expected input:
    /// - Ok(Some(RandomResponse{random_number})) - Success with random number
    /// - Ok(None) - Execution failed (contract error, compilation failed, etc.)
    /// - Err(_) - Promise system error (should never happen)
    #[private]
    pub fn on_random_result(
        &mut self,
        player: AccountId,
        choice: CoinSide,
        #[callback_result] result: Result<Option<RandomResponse>, PromiseError>,
    ) -> String {
        match result {
            // Success case: We received Some(RandomResponse)
            Ok(Some(random_response)) => {
                log!("Received random_response from OutLayer: {:?}", random_response);

                let random_number = random_response.random_number;
                log!("Random number: {}", random_number);

                // Convert to CoinSide (0 = Heads, 1 = Tails)
                let result_side = if random_number == 0 {
                    CoinSide::Heads
                } else {
                    CoinSide::Tails
                };

                // Check if player won
                if choice == result_side {
                    log!(
                        "Player {} WON! Choice: {:?}, Result: {:?}",
                        player,
                        choice,
                        result_side
                    );

                    format!(
                        "Congratulations! You won! Result: {:?}, Your choice: {:?}",
                        result_side,
                        choice
                    )
                } else {
                    log!(
                        "Player {} LOST. Choice: {:?}, Result: {:?}",
                        player,
                        choice,
                        result_side
                    );

                    format!(
                        "Sorry, you lost. Result: {:?}, Your choice: {:?}. Better luck next time!",
                        result_side,
                        choice
                    )
                }
            }

            // Failure case: OutLayer returned None (execution failed, no panic)
            Ok(None) => {
                log!("OutLayer execution failed - received None");
                env::panic_str("OutLayer execution failed")
            }

            // Promise error: This should never happen in normal operation
            Err(promise_error) => {
                log!("Promise system error: {:?}", promise_error);
                env::panic_str(&format!("Promise system error: {:?}", promise_error))
            }
        }
    }
}

/// OutLayer contract of the network this account lives on
fn default_outlayer_contract_id() -> AccountId {
    let network = if env::current_account_id().as_str().ends_with(".testnet") {
        "outlayer.testnet"
    } else {
        "outlayer.near"
    };

    network.parse().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;

    fn context(account: &str) -> VMContextBuilder {
        let account: AccountId = account.parse().unwrap();
        let mut builder = VMContextBuilder::new();
        builder
            .current_account_id(account.clone())
            .predecessor_account_id(account);
        builder
    }

    #[test]
    fn new_follows_the_network() {
        testing_env!(context("coin-toss.testnet").build());
        assert_eq!(
            CoinFlipContract::new(None).outlayer_contract_id,
            "outlayer.testnet".parse::<AccountId>().unwrap()
        );

        testing_env!(context("coin-toss.near").build());
        assert_eq!(
            CoinFlipContract::new(None).outlayer_contract_id,
            "outlayer.near".parse::<AccountId>().unwrap()
        );
    }

    #[test]
    fn new_takes_an_explicit_contract() {
        testing_env!(context("coin-toss.near").build());
        let explicit: AccountId = "outlayer.testnet".parse().unwrap();
        assert_eq!(
            CoinFlipContract::new(Some(explicit.clone())).outlayer_contract_id,
            explicit
        );
    }

    #[test]
    fn migrate_reads_the_fieldless_state() {
        testing_env!(context("coin-toss.testnet").build());
        env::state_write(&FieldlessCoinFlipContract {});

        assert_eq!(
            CoinFlipContract::migrate(None).outlayer_contract_id,
            "outlayer.testnet".parse::<AccountId>().unwrap()
        );
    }

    #[test]
    #[should_panic(expected = "Method migrate is private")]
    fn migrate_is_private() {
        let mut builder = context("coin-toss.testnet");
        builder.predecessor_account_id("mallory.testnet".parse().unwrap());
        testing_env!(builder.build());
        env::state_write(&FieldlessCoinFlipContract {});

        CoinFlipContract::migrate(None);
    }

    #[test]
    #[should_panic(expected = "No state to migrate")]
    fn migrate_refuses_an_account_with_no_state() {
        testing_env!(context("coin-toss.testnet").build());
        CoinFlipContract::migrate(None);
    }
}

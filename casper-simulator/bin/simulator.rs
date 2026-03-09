use odra::casper_types::{U256, U512};
use odra::host::{Deployer, HostEnv};
use odra::prelude::*;
use odra_modules::access::{Ownable, OwnableInitArgs};
use odra_modules::erc20::{Erc20, Erc20InitArgs};

fn main() {
    let env = odra_casper_livenet_env::env();

    // Deploy contracts
    env.set_gas(500_000_000_000);
    let owner = env.caller();
    let mut ownable = Ownable::deploy(&env, OwnableInitArgs { owner });
    let mut token = Erc20::deploy(&env, Erc20InitArgs {
        name: "TestToken".to_string(),
        symbol: "TT".to_string(),
        decimals: 2,
        initial_supply: Some(U256::from(1_000_000_000)),
    });

    write_apps_config_json(&env, &ownable, &token);

    // Account roles:
    //   exchange_1 (account 1) = Exchange Alpha  — registered exchange
    //   exchange_2 (account 2) = Exchange Beta   — registered exchange
    //   user_3     (account 3) = Regular User    — not an exchange
    //   user_4     (account 4) = Regular User 2  — not an exchange
    let exchange_1 = env.get_account(1);
    let exchange_2 = env.get_account(2);
    let user_3 = env.get_account(3);
    let user_4 = env.get_account(4);

    loop {
        // --- Native CSPR transfers ---

        // user2user: user_3 → user_4 (100 CSPR)
        env.set_caller(user_3);
        env.transfer(user_4, U512::from(100_000_000_000u64))
            .expect("user_3 -> user_4 transfer failed");

        // user2user: user_4 → user_3 (200 CSPR)
        env.set_caller(user_4);
        env.transfer(user_3, U512::from(200_000_000_000u64))
            .expect("user_4 -> user_3 transfer failed");

        // user2exchange (inflow): user_3 → exchange_1 (1000 CSPR)
        env.set_caller(user_3);
        env.transfer(exchange_1, U512::from(1_000_000_000_000u64))
            .expect("user_3 -> exchange_1 transfer failed");

        // user2exchange (inflow): user_4 → exchange_2 (500 CSPR)
        env.set_caller(user_4);
        env.transfer(exchange_2, U512::from(500_000_000_000u64))
            .expect("user_4 -> exchange_2 transfer failed");

        // exchange2user (outflow): exchange_1 → user_4 (750 CSPR)
        env.set_caller(exchange_1);
        env.transfer(user_4, U512::from(750_000_000_000u64))
            .expect("exchange_1 -> user_4 transfer failed");

        // exchange2user (outflow): exchange_2 → user_3 (300 CSPR)
        env.set_caller(exchange_2);
        env.transfer(user_3, U512::from(300_000_000_000u64))
            .expect("exchange_2 -> user_3 transfer failed");

        // --- Contract interactions ---

        // Ownable ownership cycling (generates apps.contracts events)
        env.set_caller(owner);
        ownable.try_transfer_ownership(&exchange_1).expect("Failed to transfer ownership to exchange_1");

        env.set_caller(exchange_1);
        ownable.try_transfer_ownership(&exchange_2).expect("Failed to transfer ownership to exchange_2");

        env.set_caller(exchange_2);
        ownable.try_transfer_ownership(&owner).expect("Failed to transfer ownership back to owner");

        // ERC-20 mint/burn (generates apps.contracts events)
        env.set_caller(owner);
        token.try_mint(&user_3, &U256::from(1000)).expect("Failed to mint tokens");
        token.try_burn(&user_3, &U256::from(1000)).expect("Failed to burn tokens");
    }
}


fn write_apps_config_json(env: &HostEnv, ownable: &dyn Addressable, token: &dyn Addressable) {
    // Write a single unified config file consumed by the event-router's IdentifierRegistry.
    // Format matches what load_app_identifiers() and ExchangeWalletIdentifier both expect:
    //   { "apps": [{name, topic, contract_hash}], "exchanges": {address: name} }
    // APPS_CONFIG_PATH is shared between the simulator and event-router via a Docker named volume.
    // Full address strings are written here; the event-router normalizes prefixes on load.
    let json_path = std::env::var("APPS_CONFIG_PATH")
        .unwrap_or_else(|_| "apps_config.json".to_string());

    let json = serde_json::json!({
        "apps": [
            {
                "name": "Token Contract",
                "topic": "apps.contracts",
                "contract_hash": token.address().to_string()
            },
            {
                "name": "Ownable Contract",
                "topic": "apps.contracts",
                "contract_hash": ownable.address().to_string()
            }
        ],
        "exchanges": {
            env.get_account(1).to_string(): "Exchange Alpha",
            env.get_account(2).to_string(): "Exchange Beta"
        }
    });

    std::fs::write(&json_path, json.to_string()).expect("Failed to write apps config JSON");
}

use odra::casper_types::{U256, U512};
use odra::host::Deployer;
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

    // Write deployed addresses as JSON for the event-router to read.
    // DEPLOYED_CONTRACTS_JSON_PATH allows Docker services to share via a named volume.
    // Full address strings are written here; the event-router normalizes prefixes on load.
    let json_path = std::env::var("DEPLOYED_CONTRACTS_JSON_PATH")
        .unwrap_or_else(|_| "deployed_contracts.json".to_string());
    let json = serde_json::json!({
        "contracts": {
            "Token Contract": token.address().to_string(),
            "Ownable Contract": ownable.address().to_string(),
        }
    });
    std::fs::write(&json_path, json.to_string()).expect("Failed to write deployed contracts JSON");

    // Run simulation
    env.set_gas(5_000_000_000);
    let user_1 = env.get_account(1);
    let user_2 = env.get_account(2);
    let user_3 = env.get_account(3);

    loop {
        env.set_caller(user_1);
        env.transfer(user_2, U512::from(1_000_000_000_000u64))
            .expect("Failed to transfer tokens to user 2");

        env.set_caller(user_2);
        env.transfer(user_3, U512::from(500_000_000_000u64))
            .expect("Failed to transfer tokens to user 3");

        env.set_caller(user_3);
        env.transfer(user_1, U512::from(250_000_000_000u64))
            .expect("Failed to transfer tokens back to user 1");

        env.set_caller(owner);
        ownable.try_transfer_ownership(&user_1).expect("Failed to transfer ownership to user 1");

        env.set_caller(user_1);
        ownable.try_transfer_ownership(&user_2).expect("Failed to transfer ownership to user 2");

        env.set_caller(user_2);
        ownable.try_transfer_ownership(&user_3).expect("Failed to transfer ownership to user 3");

        env.set_caller(user_3);
        ownable.try_transfer_ownership(&owner).expect("Failed to transfer ownership back to original owner");

        env.set_caller(user_1);
        token.try_mint(&user_2, &U256::from(1000)).expect("Failed to mint tokens");
        token.try_burn(&user_2, &U256::from(1000)).expect("Failed to burn tokens");
    }
}

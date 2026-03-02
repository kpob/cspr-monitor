 //! Deploys a CEP-18 contract and transfers some tokens to another address.
use odra::casper_types::U256;
use odra::host::Deployer;
use odra::prelude::*;
use odra_modules::access::{Ownable, OwnableInitArgs};
use odra_modules::erc20::{Erc20, Erc20InitArgs};

fn main() {
    let env = odra_casper_livenet_env::env();
    env.set_gas(500_000_000_000);

    let owner = env.caller();
    let ownable = Ownable::deploy(&env, OwnableInitArgs {
        owner,
    });
    let token = Erc20::deploy(&env, Erc20InitArgs {
        name: "TestToken".to_string(),
        symbol: "TT".to_string(),
        decimals: 2,
        initial_supply: Some(U256::from(1_000_000_000)),
    });

    // Write deployed addresses as JSON for both the simulator and event-router to read.
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
}

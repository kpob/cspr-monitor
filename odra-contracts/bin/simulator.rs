use odra::casper_types::{U256, U512};
use odra_modules::access::Ownable;
use odra_modules::erc20::Erc20;
use odra::prelude::*;
use odra::host::HostRefLoader;

fn main() {
    let env = odra_casper_livenet_env::env();
    env.set_gas(5_000_000_000);
    let user_1 = env.get_account(1);
    let user_2 = env.get_account(2);
    let user_3 = env.get_account(3);
    // Read deployed contract addresses from the JSON file written by the deployer.
    // DEPLOYED_CONTRACTS_JSON_PATH allows Docker services to share via a named volume.
    let json_path = std::env::var("DEPLOYED_CONTRACTS_JSON_PATH")
        .unwrap_or_else(|_| "deployed_contracts.json".to_string());
    let json_str = std::fs::read_to_string(&json_path)
        .expect("Failed to read deployed contracts JSON");
    let json: serde_json::Value = serde_json::from_str(&json_str)
        .expect("Failed to parse deployed contracts JSON");
    let contracts = &json["contracts"];

    let token_addr = Address::from_str(contracts["Token Contract"].as_str().unwrap())
        .expect("Failed to parse token address");
    let ownable_addr = Address::from_str(contracts["Ownable Contract"].as_str().unwrap())
        .expect("Failed to parse ownable address");

    let mut token = Erc20::load(&env, token_addr);
    let mut ownable = Ownable::load(&env, ownable_addr);

    let owner = ownable.get_owner();
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
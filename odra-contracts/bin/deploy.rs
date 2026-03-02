 //! Deploys a CEP-18 contract and transfers some tokens to another address.
use odra::casper_types::U256;
use odra::host::Deployer;
use odra::prelude::*;
use odra_modules::access::{Ownable, OwnableInitArgs};
use odra_modules::erc20::{Erc20, Erc20InitArgs};
use std::io::Write;

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

    // Store addresses in a file for later use in tests.
    // DEPLOYED_ADDRESSES_PATH allows Docker to write to a shared volume.
    let path = std::env::var("DEPLOYED_ADDRESSES_PATH")
        .unwrap_or_else(|_| "deployed_addresses.txt".to_string());
    let mut file = std::fs::File::create(&path).expect("Failed to create file");
    writeln!(file, "Token Contract:{}", token.address().to_string()).expect("Failed to write token address");
    writeln!(file, "Ownable Contract:{}", ownable.address().to_string()).expect("Failed to write ownable address");
}

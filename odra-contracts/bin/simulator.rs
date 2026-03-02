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
    // Read deployed contract address from file.
    // DEPLOYED_ADDRESSES_PATH allows Docker to read from a shared volume.
    let path = std::env::var("DEPLOYED_ADDRESSES_PATH")
        .unwrap_or_else(|_| "deployed_addresses.txt".to_string());
    let file = std::fs::read_to_string(&path)
        .expect("Failed to read deployed addresses");
    let addresses = file.lines().map(|line| {
        Address::from_str(line.split(':').nth(1).unwrap().trim()).expect("Failed to parse address")
    }).collect::<Vec<Address>>();

    let mut token = Erc20::load(&env, addresses[0]);
    let mut ownable = Ownable::load(&env, addresses[1]);

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
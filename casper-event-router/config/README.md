# Configuration Files

This directory contains configuration files for various identifiers in the event router.

## exchanges.json

Defines known exchange wallet addresses and their names. The format is:

```json
{
  "exchanges": {
    "address_1": "Exchange Name 1",
    "address_2": "Exchange Name 2"
  }
}
```

### Loading Priority

The application loads exchange addresses in the following order:

1. **File-based** (highest priority): Reads from the file specified in `EXCHANGE_CONFIG_PATH` environment variable, or defaults to `config/exchanges.json`
2. **Environment variable**: `EXCHANGE_ADDRESSES` in format `address1=Name1,address2=Name2`
3. **Hardcoded defaults** (lowest priority): Fallback if neither file nor env var is available

### Usage

#### Default Usage
Simply place your `exchanges.json` file in the `config/` directory. The application will automatically load it on startup.

#### Custom Path
Set the `EXCHANGE_CONFIG_PATH` environment variable to specify a different location:

```bash
export EXCHANGE_CONFIG_PATH=/path/to/your/exchanges.json
```

#### Dynamic Updates
To reload the exchange list:
1. Edit the `exchanges.json` file
2. Restart the application

### Adding New Exchanges

To add a new exchange, simply add a new entry to the `exchanges` object:

```json
{
  "exchanges": {
    "existing_address": "Existing Exchange",
    "new_exchange_address": "New Exchange Name"
  }
}
```

No code changes required!

---

## known_contracts.json

Defines known smart contract addresses (package hashes) and their names. The format is:

```json
{
  "contracts": {
    "Contract Name 1": "contract_hash_1",
    "Contract Name 2": "contract_hash_2"
  }
}
```

### Loading Priority

The application loads contract addresses in the following order:

1. **File-based** (highest priority): Reads from the file specified in `CONTRACT_CONFIG_PATH` environment variable, or defaults to `config/known_contracts.json`
2. **Environment variable**: `CONTRACT_PATTERNS` in format `Name1=hash1,Name2=hash2` or just `hash1,hash2`
3. **Empty list** (lowest priority): No contracts tracked if neither file nor env var is available

### Usage

#### Default Usage
Simply place your `known_contracts.json` file in the `config/` directory. The application will automatically load it on startup.

#### Custom Path
Set the `CONTRACT_CONFIG_PATH` environment variable to specify a different location:

```bash
export CONTRACT_CONFIG_PATH=/path/to/your/known_contracts.json
```

#### Dynamic Updates
To reload the contract list:
1. Edit the `known_contracts.json` file
2. Restart the application

### Adding New Contracts

To add a new contract, simply add a new entry to the `contracts` object:

```json
{
  "contracts": {
    "Existing Contract": "existing_hash",
    "New Contract Name": "new_contract_package_hash"
  }
}
```

The contract name will be included in the generated events, making it easier to identify which contract was interacted with.

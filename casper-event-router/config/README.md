# Configuration Files

This directory contains configuration files for the event router.

## apps_config.json

Unified configuration file that defines both per-app contract identifiers and known exchange wallet addresses.

### Format

```json
{
  "apps": [
    {
      "name": "My App",
      "topic": "apps.my-app",
      "contract_hash": "<hex contract package hash>"
    }
  ],
  "exchanges": {
    "<account address hex>": "Exchange Name"
  }
}
```

### Loading Priority

The application loads the config in the following order:

1. **File-based** (highest priority): Reads from the file specified in `APPS_CONFIG_PATH` environment variable, or defaults to `config/apps_config.json`
2. **Empty defaults** (lowest priority): Empty app list and exchange map if the file is missing or unreadable

### Usage

#### Default Usage

Place your `apps_config.json` in the `config/` directory. The application loads it automatically on startup.

#### Custom Path

```bash
export APPS_CONFIG_PATH=/path/to/your/apps_config.json
```

#### Dynamic Updates (Hot Reload)

`IdentifierRegistry::reload_all()` re-reads the file at runtime — both the `apps` array and the `exchanges` map are replaced atomically. To trigger a reload, restart the application or integrate a signal handler that calls `reload_all()`.

---

## `apps` — Per-App Contract Identifiers

Each entry in the `apps` array registers a single app matched by its contract package hash. On match, an `AppEvent` is published to the app's dedicated Kafka topic.

| Field | Description |
|---|---|
| `name` | Human-readable app name (included in `app_data.contract_name`) |
| `topic` | Kafka topic to publish matched events to |
| `contract_hash` | Contract package hash (hex). Prefixes like `hash-<hex>` are stripped automatically |

Contract transactions that do **not** match any registered app are published to `apps.unclassified`.

### Adding a New App

```json
{
  "apps": [
    {
      "name": "Existing App",
      "topic": "apps.existing",
      "contract_hash": "existing_hash"
    },
    {
      "name": "New App Name",
      "topic": "apps.new-app",
      "contract_hash": "new_contract_package_hash"
    }
  ],
  "exchanges": { }
}
```

No code changes required.

---

## `exchanges` — Exchange Wallet Identifiers

The `exchanges` map registers known exchange wallet addresses. On match (sender or transfer target), an `AppEvent` with `direction: "inflow"` or `"outflow"` is published to the `apps.exchanges` Kafka topic.

Address keys may include prefixes (e.g. `account-hash-<hex>`) — they are normalized to plain hex automatically.

### Adding a New Exchange

```json
{
  "apps": [],
  "exchanges": {
    "existing_address": "Existing Exchange",
    "new_exchange_address": "New Exchange Name"
  }
}
```

No code changes required.

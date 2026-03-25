---
name: casper-tx-analyzer
description: >
  Coding assistant for building Casper blockchain (CSPR) transaction processing applications.
  Use this skill whenever the user is writing, debugging, or designing code that handles Casper
  messages containing `raw_accepted` or `raw_processed` fields. Trigger on any mention of: Casper
  transaction parsing, deploy/transaction deserialization, raw_accepted/raw_processed handling,
  Casper message schemas, writing processors/handlers/pipelines for Casper data, typing/modeling
  Casper RPC structures, ExecutionResult, Deploy, TransactionV1, or CLValue handling. Also trigger
  when the user is working with code that touches Casper JSON data and needs to understand field
  structure, variant differences (v1 vs v2), or how to branch on schema version. This skill bundles
  the exact JSON schemas so the AI can generate accurate types, parsers, validators, and processing
  logic without guessing at field names or nesting.
---

# Casper Transaction Processing — Coding Assistant

This skill helps you write code that processes messages from a Casper blockchain application.
Each message has two relevant fields:

- **`raw_accepted`** — The transaction as accepted into the network (pre-execution).
- **`raw_processed`** — The transaction after execution, with results, costs, and state effects.

Both fields exist in two JSON schema variants (v1 and v2). Code must handle both.

## Step 1: Read the schemas

**Always read the schema files before generating any code that touches these structures.**
Do not rely on memory — the schemas are the source of truth.

```
view <skill-path>/references/raw_accepted_v1.json
view <skill-path>/references/raw_accepted_v2.json
view <skill-path>/references/raw_processed_v1.json
view <skill-path>/references/raw_processed_v2.json
```

Load all four. They are moderately sized but having the full picture prevents mismatches.

### Schema overview

| File | Title | Top-level shape |
|------|-------|-----------------|
| `raw_accepted_v1.json` | **Deploy** | `{ hash, header, payment, session, approvals }` |
| `raw_accepted_v2.json` | **TransactionV1** | `{ hash, payload, approvals }` |
| `raw_processed_v1.json` | **ExecutionResultV1** | `{ Success: {…} }` or `{ Failure: {…} }` |
| `raw_processed_v2.json` | **ExecutionResultV2** | `{ initiator, error_message, cost, consumed, limit, refund, effects, transfers, … }` |

## Step 2: Variant detection

The v1 and v2 variants have structurally distinct shapes. Use these concrete discriminators
when generating detection code:

### raw_accepted

| | v1 (Deploy) | v2 (TransactionV1) |
|---|---|---|
| **Distinguishing key** | Has `header`, `payment`, `session` | Has `payload` |
| **Sender** | `header.account` (public key string) | `payload.initiator_addr` (`{PublicKey: "…"}` or `{AccountHash: "…"}`) |
| **Gas/pricing** | `header.gas_price` (integer) | `payload.pricing_mode` (tagged union: `PaymentLimited`, `Fixed`, or `Prepaid`) |
| **Execution target** | `session` — tagged union of `ModuleBytes`, `StoredContractByHash`, `StoredContractByName`, `StoredVersionedContractByHash`, `StoredVersionedContractByName`, `Transfer` | `payload.fields.target` — `"Native"`, `{Stored: {…}}`, or `{Session: {…}}` |
| **Entry point** | Implicit in session variant (e.g. `StoredContractByHash.entry_point`) | `payload.fields.entry_point` — `"Transfer"`, `"Delegate"`, `{Custom: "name"}`, etc. |
| **Args** | `session.*.args` / `payment.*.args` — `RuntimeArgs` as `[[name, CLValue]]` | `payload.fields.args` — `{Named: RuntimeArgs}` or `{Bytesrepr: "…"}` |

**Detection logic:** Check for the presence of `payload` (v2) vs `header` + `payment` + `session` (v1).

### raw_processed

| | v1 (ExecutionResultV1) | v2 (ExecutionResultV2) |
|---|---|---|
| **Top-level shape** | Tagged union: `{Success: {…}}` or `{Failure: {…}}` | Flat object with all fields always present |
| **Success/failure** | Determined by which key is present (`Success` or `Failure`) | `error_message`: `null` = success, string = failure reason |
| **Cost** | `Success.cost` or `Failure.cost` (U512 string) | `cost` (top-level, always present) |
| **Gas detail** | Only `cost` | `limit`, `consumed`, `cost`, `refund`, `current_price` — all top-level |
| **Effects** | `Success.effect.transforms` (array of `{key, transform}` with `TransformKindV1`) | `effects` (top-level array of `{key, kind}` with `TransformV2`) |
| **Transfers** | `Success.transfers` — array of transfer address strings | `transfers` — array of `{Version1: TransferV1}` or `{Version2: TransferV2}` objects with full detail |

**Detection logic:** Check whether the object has a top-level `Success` or `Failure` key (v1)
vs. a top-level `error_message` key (v2). These are mutually exclusive.

## Step 3: Generate code

### Types and models

When generating types (TypeScript interfaces, Rust structs/enums, Python dataclasses, Go
structs, etc.), derive them **directly from the schema files**. Key rules:

- Create separate types per variant: `DeployAccepted` (v1) and `TransactionV1Accepted` (v2),
  `ExecutionResultV1` and `ExecutionResultV2`.
- Create a discriminated union at the message level:
  ```
  type RawAccepted = Deploy | TransactionV1
  type RawProcessed = ExecutionResultV1 | ExecutionResultV2
  ```
- Model `oneOf` schemas as tagged unions / sum types. Both Casper versions use this pattern
  extensively (e.g. `ExecutableDeployItem`, `PricingMode`, `TransactionTarget`,
  `TransactionEntryPoint`, `TransformKindV1`).
- `RuntimeArgs` is `Array<[string, CLValue]>` in both versions. In v2, args are wrapped in
  `{Named: RuntimeArgs}` or `{Bytesrepr: string}`.
- Large numeric values (`U512`, `U256`, `U128`) are serialized as decimal strings, not numbers.
  Type them as strings and use a big-integer library for arithmetic.
- Mark optional fields based on the schema's `required` array. Fields not listed in `required`
  but present in `properties` are optional.

### Processing pipeline pattern

When scaffolding a handler or processor, use this structure:

1. **Parse** — Deserialize the raw JSON. Handle malformed input with clear errors.
2. **Detect variant** — For `raw_accepted` and `raw_processed` independently (they can be
   different versions in the same message).
3. **Dispatch** — Route to variant-specific processing logic. Avoid giant if/else blocks;
   prefer a strategy or visitor pattern.
4. **Normalize** (optional) — If the user's app doesn't need variant-specific detail, map
   both variants into a common internal model. A reasonable common model might include:
   `hash`, `sender`, `timestamp`, `chain_name`, `entry_point`, `args`, `cost`,
   `success: boolean`, `error_message?`, `effects[]`, `transfers[]`.
5. **Extract** — Pull the fields relevant to the use case.
6. **Error handling** — Failed transactions have different shapes:
   - v1: the `Failure` branch contains `error_message` + partial effects.
   - v2: `error_message` is non-null, but all other fields (cost, effects, etc.) are still present.

### Validation

When writing validation code:

- Check that the discriminator keys exist before accessing nested fields.
- Validate `hash` fields match the expected hex pattern (`^[0-9a-fA-F]{64}$`).
- Validate numeric strings (`cost`, `amount`, `limit`, etc.) match `^[0-9]+$`.
- Validate that `oneOf` fields contain exactly one of the expected variants.
- If data matches neither v1 nor v2, surface a clear error with the actual top-level keys found.

## Casper domain context

Use this terminology correctly in generated code, comments, and variable names:

- **Deploy** (v1) / **Transaction** (v2): The unit of work submitted to the network.
- **Account**: Identified by a public key with algorithm prefix (`01` = Ed25519, `02` = Secp256k1).
- **InitiatorAddr** (v2): Can be either `{PublicKey: "…"}` or `{AccountHash: "…"}`.
- **Entry point**: The smart contract function being called.
- **Payment** + **Session** (v1 only): Two execution phases. v2 merges these into `payload.fields`.
- **PricingMode** (v2 only): `PaymentLimited`, `Fixed`, or `Prepaid`. Replaces v1's simple `gas_price` integer.
- **Transforms / Effects**: State changes produced by execution. v1 uses `TransformKindV1`
  (rich tagged union with ~18 variants). v2 simplifies to `TransformV2` with `{key, kind}`.
- **Motes**: Smallest CSPR unit (1 CSPR = 10^9 motes). Gas costs are in motes.
- **Era**: Consensus time period, relevant for staking/delegation.
- **Native entry points** (v2): `Transfer`, `Delegate`, `Undelegate`, `Redelegate`, `AddBid`,
  `WithdrawBid`, `ActivateBid`, `ChangeBidPublicKey`, `Burn`, `AddReservations`,
  `CancelReservations`. Plus `Call` and `{Custom: "name"}`.

## What this skill does NOT cover

- Blockchain node operation or RPC client code (this is about processing messages the app
  already receives, not fetching them).
- Smart contract development (Casper contract SDK / CEP standards).
- Key management or cryptographic signing.

If the user's question falls into these areas, help with general knowledge but note the
bundled schemas won't be directly relevant.
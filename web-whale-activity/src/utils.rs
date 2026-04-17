pub(crate) fn parse_amount(args: &serde_json::Value) -> u64 {
    args.get("amount")
        .and_then(|v| v.as_str().or_else(|| v.as_u64().map(|_| "")).and_then(|s| {
            if s.is_empty() { v.as_u64() } else { s.parse().ok() }
        }))
        .unwrap_or(0)
}
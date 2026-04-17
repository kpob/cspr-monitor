pub fn shorten_address(addr: &str) -> String {
    if addr.len() > 16 {
        format!("{}..{}", &addr[..8], &addr[addr.len() - 6..])
    } else {
        addr.to_string()
    }
}
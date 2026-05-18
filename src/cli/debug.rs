pub fn cli_debug_enabled() -> bool {
    ["DEBUG", "STACKER_DEBUG"].iter().any(|key| {
        std::env::var(key)
            .map(|value| {
                matches!(
                    value.to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    })
}

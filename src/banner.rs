/// Display a banner with version and useful information
pub fn print_banner() {
    let version = env!("CARGO_PKG_VERSION");
    let name = env!("CARGO_PKG_NAME");

    let git_hash = option_env!("STACKER_GIT_SHORT_HASH").unwrap_or("unknown");

    let banner = format!(
        r#"
        _              | |                
  ___ _| |_ _____  ____| |  _ _____  ____ 
 /___|_   _|____ |/ ___) |_/ ) ___ |/ ___)
|___ | | |_/ ___ ( (___|  _ (| ____| |    
(___/   \__)_____|\____)_| \_)_____)_|    

──────────────────────────────────────────
  {}
  Version: {} (git: {})
  Edition: {}  
─────────────────────────────────────────

"#,
        capitalize(name),
        version,
        git_hash,
        "2021"
    );

    println!("{}", banner);
}

/// Display startup information
pub fn print_startup_info(host: &str, port: u16) {
    let info = format!(
        r#"
📋 Configuration Loaded
  🌐 Server Address: http://{}:{}
  📦 Ready to accept connections
  
"#,
        host, port
    );

    println!("{}", info);
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capitalize() {
        assert_eq!(capitalize("stacker"), "Stacker");
        assert_eq!(capitalize("hello"), "Hello");
        assert_eq!(capitalize(""), "");
    }
}

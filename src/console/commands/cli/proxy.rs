use crate::console::commands::CallableTrait;

/// `stacker proxy add <domain> [--upstream <host:port>] [--ssl auto|manual|off]`
///
/// Adds a reverse-proxy entry for the given domain.
pub struct ProxyAddCommand {
    pub domain: String,
    pub upstream: Option<String>,
    pub ssl: Option<String>,
}

impl ProxyAddCommand {
    pub fn new(domain: String, upstream: Option<String>, ssl: Option<String>) -> Self {
        Self {
            domain,
            upstream,
            ssl,
        }
    }
}

impl CallableTrait for ProxyAddCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("stacker proxy add — not yet implemented");
        Ok(())
    }
}

/// `stacker proxy detect`
///
/// Scans running containers for an existing reverse-proxy (nginx, traefik, etc.)
/// and reports what was found.
pub struct ProxyDetectCommand;

impl ProxyDetectCommand {
    pub fn new() -> Self {
        Self
    }
}

impl CallableTrait for ProxyDetectCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("stacker proxy detect — not yet implemented");
        Ok(())
    }
}

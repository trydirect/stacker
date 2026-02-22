use crate::console::commands::CallableTrait;

/// `stacker status [--json] [--watch]`
///
/// Shows the current deployment status (containers, health, ports).
pub struct StatusCommand {
    pub json: bool,
    pub watch: bool,
}

impl StatusCommand {
    pub fn new(json: bool, watch: bool) -> Self {
        Self { json, watch }
    }
}

impl CallableTrait for StatusCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("stacker status — not yet implemented");
        Ok(())
    }
}

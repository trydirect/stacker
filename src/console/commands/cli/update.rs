use crate::console::commands::CallableTrait;

/// `stacker update [--channel stable|beta]`
///
/// Checks for updates and self-updates the stacker binary.
pub struct UpdateCommand {
    pub channel: Option<String>,
}

impl UpdateCommand {
    pub fn new(channel: Option<String>) -> Self {
        Self { channel }
    }
}

impl CallableTrait for UpdateCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("stacker update — not yet implemented");
        Ok(())
    }
}

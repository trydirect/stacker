use crate::console::commands::CallableTrait;

/// `stacker destroy [--volumes] [--confirm]`
///
/// Tears down the deployed stack and optionally removes volumes.
pub struct DestroyCommand {
    pub volumes: bool,
    pub confirm: bool,
}

impl DestroyCommand {
    pub fn new(volumes: bool, confirm: bool) -> Self {
        Self { volumes, confirm }
    }
}

impl CallableTrait for DestroyCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("stacker destroy — not yet implemented");
        Ok(())
    }
}

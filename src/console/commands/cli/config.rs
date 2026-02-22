use crate::console::commands::CallableTrait;

/// `stacker config validate [--file stacker.yml]`
///
/// Validates a stacker.yml configuration file.
pub struct ConfigValidateCommand {
    pub file: Option<String>,
}

impl ConfigValidateCommand {
    pub fn new(file: Option<String>) -> Self {
        Self { file }
    }
}

impl CallableTrait for ConfigValidateCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("stacker config validate — not yet implemented");
        Ok(())
    }
}

/// `stacker config show [--file stacker.yml]`
///
/// Displays the resolved configuration (with env vars substituted).
pub struct ConfigShowCommand {
    pub file: Option<String>,
}

impl ConfigShowCommand {
    pub fn new(file: Option<String>) -> Self {
        Self { file }
    }
}

impl CallableTrait for ConfigShowCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("stacker config show — not yet implemented");
        Ok(())
    }
}

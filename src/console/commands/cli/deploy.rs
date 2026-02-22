use crate::console::commands::CallableTrait;

/// `stacker deploy [--target local|cloud|server] [--file stacker.yml] [--dry-run] [--force-rebuild]`
///
/// Generates Dockerfile + docker-compose from stacker.yml, then
/// deploys using the appropriate strategy (local, cloud, or server).
pub struct DeployCommand {
    pub target: Option<String>,
    pub file: Option<String>,
    pub dry_run: bool,
    pub force_rebuild: bool,
}

impl DeployCommand {
    pub fn new(
        target: Option<String>,
        file: Option<String>,
        dry_run: bool,
        force_rebuild: bool,
    ) -> Self {
        Self {
            target,
            file,
            dry_run,
            force_rebuild,
        }
    }
}

impl CallableTrait for DeployCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("stacker deploy — not yet implemented");
        Ok(())
    }
}

use crate::console::commands::CallableTrait;

/// `stacker logs [--service <name>] [--follow] [--tail <n>] [--since <duration>]`
///
/// Shows container logs for the deployed stack (delegates to docker compose logs).
pub struct LogsCommand {
    pub service: Option<String>,
    pub follow: bool,
    pub tail: Option<u32>,
    pub since: Option<String>,
}

impl LogsCommand {
    pub fn new(
        service: Option<String>,
        follow: bool,
        tail: Option<u32>,
        since: Option<String>,
    ) -> Self {
        Self {
            service,
            follow,
            tail,
            since,
        }
    }
}

impl CallableTrait for LogsCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("stacker logs — not yet implemented");
        Ok(())
    }
}

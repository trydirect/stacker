use crate::console::commands::CallableTrait;

/// `stacker ai ask "<question>" [--context <file>]`
///
/// Sends a question to the configured AI provider for assistance
/// with Dockerfile, docker-compose, or deployment troubleshooting.
pub struct AiAskCommand {
    pub question: String,
    pub context: Option<String>,
}

impl AiAskCommand {
    pub fn new(question: String, context: Option<String>) -> Self {
        Self { question, context }
    }
}

impl CallableTrait for AiAskCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("stacker ai ask — not yet implemented");
        Ok(())
    }
}

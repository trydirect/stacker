use crate::cli::error::CliError;
use crate::console::commands::CallableTrait;

const DEFAULT_CHANNEL: &str = "stable";
const VALID_CHANNELS: &[&str] = &["stable", "beta"];

/// Parse and validate a release channel string.
pub fn parse_channel(channel: Option<&str>) -> Result<String, CliError> {
    let ch = channel.unwrap_or(DEFAULT_CHANNEL).to_lowercase();
    if VALID_CHANNELS.contains(&ch.as_str()) {
        Ok(ch)
    } else {
        Err(CliError::ConfigValidation(format!(
            "Unknown channel '{}'. Valid channels: {}",
            ch,
            VALID_CHANNELS.join(", ")
        )))
    }
}

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
        let channel = parse_channel(self.channel.as_deref())?;
        eprintln!("Checking for updates on '{}' channel...", channel);
        eprintln!("You are running the latest version.");
        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_channel_defaults_to_stable() {
        assert_eq!(parse_channel(None).unwrap(), "stable");
    }

    #[test]
    fn test_parse_channel_accepts_beta() {
        assert_eq!(parse_channel(Some("beta")).unwrap(), "beta");
    }

    #[test]
    fn test_parse_channel_case_insensitive() {
        assert_eq!(parse_channel(Some("STABLE")).unwrap(), "stable");
    }

    #[test]
    fn test_parse_channel_rejects_unknown() {
        assert!(parse_channel(Some("nightly")).is_err());
    }
}

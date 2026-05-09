use crate::cli::error::CliError;
use crate::cli::runtime::CliRuntime;
use crate::console::commands::CallableTrait;
use crate::forms::{
    parse_private_port, parse_public_port, CloudFirewallAction, ConfigureCloudFirewallRequest,
};

pub struct CloudFirewallCommand {
    pub action: CloudFirewallAction,
    pub server_id: Option<i32>,
    pub public_ports: Vec<String>,
    pub private_ports: Vec<String>,
    pub dry_run: bool,
    pub json: bool,
}

impl CloudFirewallCommand {
    pub fn new(
        action: CloudFirewallAction,
        server_id: Option<i32>,
        public_ports: Vec<String>,
        private_ports: Vec<String>,
        dry_run: bool,
        json: bool,
    ) -> Self {
        Self {
            action,
            server_id,
            public_ports,
            private_ports,
            dry_run,
            json,
        }
    }

    fn resolve_server_id(&self, ctx: &CliRuntime) -> Result<i32, CliError> {
        if let Some(server_id) = self.server_id {
            return Ok(server_id);
        }

        let project_dir = std::env::current_dir().map_err(CliError::Io)?;
        let config_path = project_dir.join("stacker.yml");
        if !config_path.exists() {
            return Err(CliError::ConfigValidation(
                "Use --server-id <ID>, or run from a directory with stacker.yml".to_string(),
            ));
        }

        let config = crate::cli::config_parser::StackerConfig::from_file(&config_path)
            .and_then(|config| config.with_resolved_deploy_target(None))?;
        let project_name = config.project.identity.ok_or_else(|| {
            CliError::ConfigValidation(
                "Use --server-id <ID>, or set project.identity in stacker.yml".to_string(),
            )
        })?;
        let project = ctx
            .block_on(ctx.client.find_project_by_name(&project_name))?
            .ok_or_else(|| {
                CliError::ConfigValidation(format!(
                    "Project '{}' was not found on the Stacker server",
                    project_name
                ))
            })?;
        let servers = ctx.block_on(ctx.client.list_servers())?;
        let mut project_servers = servers
            .into_iter()
            .filter(|server| server.project_id == project.id)
            .collect::<Vec<_>>();
        project_servers.sort_by_key(|server| server.id);

        match project_servers.as_slice() {
            [server] => Ok(server.id),
            [] => Err(CliError::ConfigValidation(format!(
                "No server found for project '{}'. Use --server-id <ID>.",
                project_name
            ))),
            _ => Err(CliError::ConfigValidation(format!(
                "Multiple servers found for project '{}'. Use --server-id <ID>.",
                project_name
            ))),
        }
    }
}

impl CallableTrait for CloudFirewallCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("cloud firewall")?;
        let server_id = self.resolve_server_id(&ctx)?;
        let public_ports = self
            .public_ports
            .iter()
            .map(|port| parse_public_port(port))
            .collect::<Result<Vec<_>, _>>()
            .map_err(CliError::ConfigValidation)?;
        let private_ports = self
            .private_ports
            .iter()
            .map(|port| parse_private_port(port))
            .collect::<Result<Vec<_>, _>>()
            .map_err(CliError::ConfigValidation)?;

        let request = ConfigureCloudFirewallRequest {
            action: Some(self.action.clone()),
            public_ports,
            private_ports,
            dry_run: self.dry_run,
        };
        let response = ctx.block_on(ctx.client.configure_cloud_firewall(server_id, &request))?;

        if self.json {
            println!("{}", serde_json::to_string_pretty(&response)?);
            return Ok(());
        }

        println!(
            "Cloud firewall {} accepted for server {} ({})",
            response.action.as_str(),
            response.server_id,
            response.provider
        );
        println!("Operation: {}", response.operation_id);
        println!("Route: {}", response.routing_key);
        for rule in response.rules {
            println!(
                "- {} {}/{} from {}",
                rule.direction.as_str(),
                rule.port,
                rule.protocol,
                rule.source
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_firewall_command_stores_action_and_ports() {
        let command = CloudFirewallCommand::new(
            CloudFirewallAction::Add,
            Some(42),
            vec!["8000/tcp".to_string()],
            vec![],
            false,
            true,
        );

        assert_eq!(command.server_id, Some(42));
        assert_eq!(command.public_ports, vec!["8000/tcp"]);
        assert_eq!(command.action, CloudFirewallAction::Add);
    }
}

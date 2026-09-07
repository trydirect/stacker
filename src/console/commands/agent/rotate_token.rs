use crate::configuration::get_configuration;
use crate::services::agent_dispatcher;
use actix_web::rt;
use sqlx::PgPool;

pub struct RotateTokenCommand {
    pub deployment_hash: String,
}

impl RotateTokenCommand {
    /// No token parameter: the value is minted server-side. An
    /// operator-supplied secret would make the stored digest a rainbow-table
    /// target, and the old `--new-token` wrote to Vault without touching the
    /// database, which now means locking the agent out.
    pub fn new(deployment_hash: String) -> Self {
        Self { deployment_hash }
    }
}

impl crate::console::commands::CallableTrait for RotateTokenCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let deployment_hash = self.deployment_hash.clone();

        rt::System::new().block_on(async move {
            let settings = get_configuration().expect("Failed to read configuration.");
            let vault = crate::helpers::VaultClient::new(&settings.vault);

            let db_pool = PgPool::connect(&settings.database.connection_string())
                .await
                .expect("Failed to connect to database.");

            let token = agent_dispatcher::rotate_token(&db_pool, &vault, &deployment_hash)
                .await
                .map_err(|e| {
                    eprintln!("Rotate token failed: {}", e);
                    e
                })?;

            println!(
                "Rotated agent token for deployment_hash {}.\n\
                 The agent adopts it from Vault on its next refresh.\n\
                 Token: {}",
                deployment_hash, token
            );

            Ok(())
        })
    }
}

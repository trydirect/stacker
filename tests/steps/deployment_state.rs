use cucumber::when;

use super::StepWorld;

#[when(regex = r#"^I get deployment state for "([^"]+)"$"#)]
async fn get_deployment_state(world: &mut StepWorld, deployment_hash: String) {
    world
        .get(&format!("/api/v1/deployments/{}/state", deployment_hash))
        .await;
}

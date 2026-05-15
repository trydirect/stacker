use cucumber::when;

use super::StepWorld;

#[when("I request the execution stream for the instance")]
async fn when_get_execution_stream(world: &mut StepWorld) {
    let instance_id = world
        .stored_ids
        .get("dag_instance_id")
        .expect("No dag_instance_id stored");

    let path = format!("/api/v1/pipes/instances/{}/stream", instance_id);
    world.get(&path).await;
}

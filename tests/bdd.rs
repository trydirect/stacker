mod steps;

use cucumber::World as _;
use steps::StepWorld;

#[tokio::main]
async fn main() {
    if !steps::ensure_shared_app_available().await {
        eprintln!("Skipping BDD tests: failed to start test server (is PostgreSQL running?)");
        return;
    }

    StepWorld::cucumber()
        .fail_on_skipped()
        .run_and_exit("tests/features")
        .await;
}

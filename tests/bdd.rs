mod steps;

use cucumber::World as _;
use steps::StepWorld;

#[tokio::main]
async fn main() {
    StepWorld::cucumber()
        .fail_on_skipped()
        .run_and_exit("tests/features")
        .await;
}

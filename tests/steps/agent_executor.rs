use crate::steps::StepWorld;
use cucumber::{given, then, when};
use serde_json::{json, Value as JsonValue};
use stacker::models::agent_protocol::{
    routing, RetryPolicy, StepCommand, StepResultMsg, StepStatus,
};
use stacker::services::resilience_engine::{CircuitBreakerConfig, InMemoryCircuitBreaker};
use stacker::services::step_executor;
use std::time::Duration;
use uuid::Uuid;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Protocol Serialization Steps
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[given(expr = "a StepCommand with step_type {string} and config:")]
async fn given_step_command(
    world: &mut StepWorld,
    step: &cucumber::gherkin::Step,
    step_type: String,
) {
    let config_str = step.docstring.as_ref().expect("Missing docstring (config)");
    let config: JsonValue = serde_json::from_str(config_str.trim()).expect("Invalid JSON config");
    let cmd = StepCommand::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        format!("test_{}", step_type),
        step_type,
        config.clone(),
        json!({}),
        Uuid::new_v4(),
        "test-deploy".to_string(),
    );
    let serialized = serde_json::to_string(&cmd).unwrap();
    world
        .stored_ids
        .insert("step_command_json".to_string(), serialized);
    world.stored_ids.insert(
        "step_config".to_string(),
        serde_json::to_string(&config).unwrap(),
    );
}

#[given(expr = "the step input is:")]
async fn given_step_input(world: &mut StepWorld, step: &cucumber::gherkin::Step) {
    let input_str = step.docstring.as_ref().expect("Missing docstring (input)");
    world
        .stored_ids
        .insert("step_input".to_string(), input_str.trim().to_string());
}

#[given(expr = "a successful StepResultMsg with output:")]
async fn given_success_result(world: &mut StepWorld, step: &cucumber::gherkin::Step) {
    let output_str = step.docstring.as_ref().expect("Missing docstring (output)");
    let output: JsonValue = serde_json::from_str(output_str.trim()).expect("Invalid JSON output");
    let result = StepResultMsg::success(Uuid::new_v4(), Uuid::new_v4(), output, 150);
    let serialized = serde_json::to_string(&result).unwrap();
    world
        .stored_ids
        .insert("step_result_json".to_string(), serialized);
}

#[given(expr = "a failed StepResultMsg with error {string}")]
async fn given_failure_result(world: &mut StepWorld, error: String) {
    let result = StepResultMsg::failure(Uuid::new_v4(), Uuid::new_v4(), error, 500);
    let serialized = serde_json::to_string(&result).unwrap();
    world
        .stored_ids
        .insert("step_result_json".to_string(), serialized);
}

#[when("the command is serialized to JSON and back")]
async fn when_command_roundtrip(world: &mut StepWorld) {
    let json_str = world.stored_ids.get("step_command_json").unwrap().clone();
    let deserialized: StepCommand =
        serde_json::from_str(&json_str).expect("Deserialization failed");
    let reserialized = serde_json::to_string(&deserialized).unwrap();
    world
        .stored_ids
        .insert("step_command_roundtrip".to_string(), reserialized);
}

#[when("the result is serialized to JSON and back")]
async fn when_result_roundtrip(world: &mut StepWorld) {
    let json_str = world.stored_ids.get("step_result_json").unwrap().clone();
    let deserialized: StepResultMsg =
        serde_json::from_str(&json_str).expect("Deserialization failed");
    let reserialized = serde_json::to_string(&deserialized).unwrap();
    world
        .stored_ids
        .insert("step_result_roundtrip".to_string(), reserialized);
}

#[then("the deserialized command should match the original")]
async fn then_command_matches(world: &mut StepWorld) {
    let original = world.stored_ids.get("step_command_json").unwrap();
    let roundtrip = world.stored_ids.get("step_command_roundtrip").unwrap();
    let orig_val: JsonValue = serde_json::from_str(original).unwrap();
    let rt_val: JsonValue = serde_json::from_str(roundtrip).unwrap();
    assert_eq!(orig_val["step_type"], rt_val["step_type"]);
    assert_eq!(orig_val["step_name"], rt_val["step_name"]);
    assert_eq!(orig_val["config"], rt_val["config"]);
    assert_eq!(orig_val["deployment_hash"], rt_val["deployment_hash"]);
}

#[then(expr = "the deserialized result status should be {string}")]
async fn then_result_status(world: &mut StepWorld, expected: String) {
    let json_str = world
        .stored_ids
        .get("step_result_roundtrip")
        .or_else(|| world.stored_ids.get("step_result_json"))
        .unwrap();
    let result: StepResultMsg = serde_json::from_str(json_str).unwrap();
    assert_eq!(result.status.to_string(), expected);
}

#[then("the deserialized result should have output data")]
async fn then_result_has_output(world: &mut StepWorld) {
    let json_str = world.stored_ids.get("step_result_roundtrip").unwrap();
    let result: StepResultMsg = serde_json::from_str(json_str).unwrap();
    assert!(
        result.output_data.is_some(),
        "Expected output data to be present"
    );
}

#[then(expr = "the deserialized result error should be {string}")]
async fn then_result_error(world: &mut StepWorld, expected: String) {
    let json_str = world.stored_ids.get("step_result_roundtrip").unwrap();
    let result: StepResultMsg = serde_json::from_str(json_str).unwrap();
    assert_eq!(result.error.as_deref(), Some(expected.as_str()));
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Step Execution Steps
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[when("the step is executed via step_executor")]
async fn when_execute_step(world: &mut StepWorld) {
    let cmd_json = world.stored_ids.get("step_command_json").unwrap();
    let cmd: StepCommand = serde_json::from_str(cmd_json).unwrap();

    let input = world
        .stored_ids
        .get("step_input")
        .map(|s| serde_json::from_str(s).unwrap())
        .unwrap_or(cmd.input_data.clone());

    match step_executor::execute_step(&cmd.step_type, &cmd.config, &input).await {
        Ok(output) => {
            world.stored_ids.insert(
                "exec_output".to_string(),
                serde_json::to_string(&output).unwrap(),
            );
            world
                .stored_ids
                .insert("exec_success".to_string(), "true".to_string());
        }
        Err(err) => {
            world.stored_ids.insert("exec_error".to_string(), err);
            world
                .stored_ids
                .insert("exec_success".to_string(), "false".to_string());
        }
    }
}

#[then("the execution should succeed")]
async fn then_exec_success(world: &mut StepWorld) {
    let success = world.stored_ids.get("exec_success").unwrap();
    assert_eq!(
        success,
        "true",
        "Expected execution to succeed but got error: {:?}",
        world.stored_ids.get("exec_error")
    );
}

#[then(expr = "the execution should fail with error containing {string}")]
async fn then_exec_fail_with(world: &mut StepWorld, expected: String) {
    let success = world.stored_ids.get("exec_success").unwrap();
    assert_eq!(success, "false", "Expected execution to fail");
    let error = world.stored_ids.get("exec_error").unwrap();
    assert!(
        error.contains(&expected),
        "Error '{}' does not contain '{}'",
        error,
        expected
    );
}

#[then(expr = "the output should contain key {string} with value {string}")]
async fn then_output_contains(world: &mut StepWorld, key: String, value: String) {
    let output_str = world
        .stored_ids
        .get("exec_output")
        .expect("No execution output");
    let output: JsonValue = serde_json::from_str(output_str).unwrap();
    let actual = &output[&key];
    // Handle bool/string/number comparisons
    let matches = match actual {
        JsonValue::Bool(b) => b.to_string() == value,
        JsonValue::Number(n) => n.to_string() == value,
        JsonValue::String(s) => s == &value,
        other => other.to_string().trim_matches('"') == value,
    };
    assert!(
        matches,
        "output[{}] = {:?}, expected {:?}",
        key, actual, value
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Circuit Breaker Steps
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[given(expr = "a circuit breaker with failure_threshold {int} and recovery_timeout {int}")]
async fn given_circuit_breaker(world: &mut StepWorld, threshold: u32, timeout: u64) {
    let cb = InMemoryCircuitBreaker::new(CircuitBreakerConfig {
        failure_threshold: threshold,
        recovery_timeout: Duration::from_secs(timeout),
        half_open_max_requests: 1,
    });
    world.circuit_breaker = Some(cb);
}

#[when(expr = "I record {int} consecutive failures")]
async fn when_record_failures(world: &mut StepWorld, count: u32) {
    let cb = world
        .circuit_breaker
        .as_mut()
        .expect("No circuit breaker initialized");
    for _ in 0..count {
        cb.record_failure();
    }
}

#[then(expr = "the circuit breaker should be in {string} state")]
async fn then_cb_state(world: &mut StepWorld, expected: String) {
    let cb = world.circuit_breaker.as_mut().expect("No circuit breaker");
    // Call allows_request to trigger potential state transition (Open→HalfOpen)
    let _ = cb.allows_request();
    let state = format!("{:?}", cb.state()).to_lowercase();
    assert!(
        state.contains(&expected),
        "CB state '{}' does not match '{}'",
        state,
        expected
    );
}

#[then("the circuit breaker should reject requests")]
async fn then_cb_rejects(world: &mut StepWorld) {
    let cb = world.circuit_breaker.as_mut().expect("No circuit breaker");
    assert!(
        !cb.allows_request(),
        "Expected circuit breaker to reject requests"
    );
}

#[then("the circuit breaker should allow requests")]
async fn then_cb_allows(world: &mut StepWorld) {
    let cb = world.circuit_breaker.as_mut().expect("No circuit breaker");
    assert!(
        cb.allows_request(),
        "Expected circuit breaker to allow requests"
    );
}

#[when(expr = "I wait {int} seconds for recovery")]
async fn when_wait_recovery(world: &mut StepWorld, seconds: u64) {
    tokio::time::sleep(Duration::from_secs(seconds)).await;
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Resilience Steps
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[given(expr = "a retry policy with max_retries {int} and backoff_base_ms {int}")]
async fn given_retry_policy(world: &mut StepWorld, max_retries: u32, backoff_base_ms: u64) {
    world
        .stored_ids
        .insert("retry_max".to_string(), max_retries.to_string());
    world
        .stored_ids
        .insert("retry_backoff".to_string(), backoff_base_ms.to_string());
}

#[when("the step is executed with resilience")]
async fn when_execute_with_resilience(world: &mut StepWorld) {
    let cmd_json = world.stored_ids.get("step_command_json").unwrap();
    let cmd: StepCommand = serde_json::from_str(cmd_json).unwrap();

    let max_retries: u32 = world
        .stored_ids
        .get("retry_max")
        .unwrap_or(&"3".to_string())
        .parse()
        .unwrap();
    let backoff_base_ms: u64 = world
        .stored_ids
        .get("retry_backoff")
        .unwrap_or(&"10".to_string())
        .parse()
        .unwrap();

    let retry_policy = RetryPolicy {
        max_retries,
        backoff_base_ms,
        backoff_max_ms: backoff_base_ms * 10,
    };

    let mut cb = InMemoryCircuitBreaker::new(CircuitBreakerConfig::default());

    use stacker::services::resilience_engine::execute_with_resilience;
    match execute_with_resilience(
        &cmd.step_type,
        &cmd.config,
        &cmd.input_data,
        &retry_policy,
        &mut cb,
    )
    .await
    {
        Ok(output) => {
            world.stored_ids.insert(
                "exec_output".to_string(),
                serde_json::to_string(&output).unwrap(),
            );
            world
                .stored_ids
                .insert("exec_success".to_string(), "true".to_string());
        }
        Err(err) => {
            world.stored_ids.insert("exec_error".to_string(), err);
            world
                .stored_ids
                .insert("exec_success".to_string(), "false".to_string());
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Routing Steps
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[given(expr = "a deployment hash {string}")]
async fn given_deployment_hash(world: &mut StepWorld, hash: String) {
    world.stored_ids.insert("deployment_hash".to_string(), hash);
}

#[then(expr = "the execute routing key should be {string}")]
async fn then_execute_key(world: &mut StepWorld, expected: String) {
    let hash = world.stored_ids.get("deployment_hash").unwrap();
    assert_eq!(routing::execute_key(hash), expected);
}

#[then(expr = "the result routing key should be {string}")]
async fn then_result_key(world: &mut StepWorld, expected: String) {
    let hash = world.stored_ids.get("deployment_hash").unwrap();
    assert_eq!(routing::result_key(hash), expected);
}

#[then(expr = "the agent queue name should be {string}")]
async fn then_agent_queue(world: &mut StepWorld, expected: String) {
    let hash = world.stored_ids.get("deployment_hash").unwrap();
    assert_eq!(routing::agent_queue(hash), expected);
}

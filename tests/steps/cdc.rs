use cucumber::{given, then, when};
use serde_json::json;
use uuid::Uuid;

use crate::steps::StepWorld;
use stacker::models::cdc::{CdcChangeEvent, CdcOperation, CdcTriggerConfig, routing};
use stacker::services::step_executor;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// CDC Event Construction
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[given(regex = r#"^a CDC change event for table "([^"]+)" with operation "([^"]+)"$"#)]
async fn given_cdc_event(world: &mut StepWorld, table: String, operation: String) {
    let op = CdcOperation::from_str(&operation).expect("Invalid CDC operation");
    let event = CdcChangeEvent::new(
        Uuid::new_v4(),
        "public".to_string(),
        table,
        op,
        None,
        None,
        1,
        "0/1".to_string(),
    );
    world.cdc_event = Some(event);
}

#[given("the event has after data:")]
async fn given_event_after_data(world: &mut StepWorld, step: &cucumber::gherkin::Step) {
    let data: serde_json::Value = serde_json::from_str(
        step.docstring.as_ref().expect("Missing docstring"),
    )
    .expect("Invalid JSON in docstring");
    if let Some(ref mut event) = world.cdc_event {
        event.after = Some(data);
    }
}

#[given("the event has before data:")]
async fn given_event_before_data(world: &mut StepWorld, step: &cucumber::gherkin::Step) {
    let data: serde_json::Value = serde_json::from_str(
        step.docstring.as_ref().expect("Missing docstring"),
    )
    .expect("Invalid JSON in docstring");
    if let Some(ref mut event) = world.cdc_event {
        event.before = Some(data);
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// CDC Event Assertions
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[then("the event row_data should match the after data")]
async fn then_row_data_matches_after(world: &mut StepWorld) {
    let event = world.cdc_event.as_ref().expect("No CDC event");
    assert_eq!(event.row_data(), event.after.as_ref());
}

#[then("the event row_data should match the before data")]
async fn then_row_data_matches_before(world: &mut StepWorld) {
    let event = world.cdc_event.as_ref().expect("No CDC event");
    assert_eq!(event.row_data(), event.before.as_ref());
}

#[then("the event before data should be null")]
async fn then_before_is_null(world: &mut StepWorld) {
    let event = world.cdc_event.as_ref().expect("No CDC event");
    assert!(event.before.is_none());
}

#[then("the event before data should not be null")]
async fn then_before_is_not_null(world: &mut StepWorld) {
    let event = world.cdc_event.as_ref().expect("No CDC event");
    assert!(event.before.is_some());
}

#[then("the event after data should be null")]
async fn then_after_is_null(world: &mut StepWorld) {
    let event = world.cdc_event.as_ref().expect("No CDC event");
    assert!(event.after.is_none());
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// CDC Operation Parsing
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[then(regex = r#"^CDC operation "([^"]+)" should parse to (Insert|Update|Delete)$"#)]
async fn then_operation_parses(_world: &mut StepWorld, input: String, expected: String) {
    let parsed = CdcOperation::from_str(&input).expect("Should parse");
    let expected_op = match expected.as_str() {
        "Insert" => CdcOperation::Insert,
        "Update" => CdcOperation::Update,
        "Delete" => CdcOperation::Delete,
        _ => panic!("Unknown expected operation: {}", expected),
    };
    assert_eq!(parsed, expected_op);
}

#[then(regex = r#"^CDC operation "([^"]+)" should not parse$"#)]
async fn then_operation_does_not_parse(_world: &mut StepWorld, input: String) {
    assert!(CdcOperation::from_str(&input).is_none(), "Expected None for '{}'", input);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// CDC Pipe Payload
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[when("I convert the event to a pipe payload")]
async fn when_convert_to_payload(world: &mut StepWorld) {
    let event = world.cdc_event.as_ref().expect("No CDC event");
    world.cdc_payload = Some(event.to_pipe_payload());
}

#[then(regex = r#"^the payload should contain field "([^"]+)" with value "([^"]+)"$"#)]
async fn then_payload_contains_field(world: &mut StepWorld, field: String, value: String) {
    let payload = world.cdc_payload.as_ref().expect("No CDC payload");
    assert_eq!(
        payload[&field].as_str().unwrap_or_default(),
        value,
        "Field '{}' mismatch",
        field,
    );
}

#[then(regex = r#"^the payload "([^"]+)" should have key "([^"]+)"$"#)]
async fn then_payload_nested_has_key(world: &mut StepWorld, parent: String, key: String) {
    let payload = world.cdc_payload.as_ref().expect("No CDC payload");
    assert!(
        payload[&parent].get(&key).is_some(),
        "Expected key '{}' in payload.{}",
        key,
        parent,
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// CDC Source Step Execution
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[given(regex = r#"^a DAG step of type "([^"]+)" with config:$"#)]
async fn given_dag_step_with_config(world: &mut StepWorld, step_type: String, step: &cucumber::gherkin::Step) {
    let config: serde_json::Value = serde_json::from_str(
        step.docstring.as_ref().expect("Missing docstring"),
    )
    .expect("Invalid JSON in docstring");
    world.stored_ids.insert("step_type".to_string(), step_type);
    world.stored_ids.insert("step_config".to_string(), config.to_string());
}

#[when("I execute the step with empty input")]
async fn when_execute_step(world: &mut StepWorld) {
    let step_type = world.stored_ids.get("step_type").expect("No step_type").clone();
    let config: serde_json::Value = serde_json::from_str(
        world.stored_ids.get("step_config").expect("No step_config"),
    )
    .expect("Invalid config JSON");
    let result = step_executor::execute_step(&step_type, &config, &json!({}))
        .await
        .expect("Step execution failed");
    world.response_json = Some(result);
}

#[then(regex = r#"^the step result should contain "([^"]+)" as true$"#)]
async fn then_step_result_is_true(world: &mut StepWorld, field: String) {
    let result = world.response_json.as_ref().expect("No step result");
    assert_eq!(result[&field], true, "Expected {} to be true", field);
}

#[then(regex = r#"^the step result should contain "([^"]+)" as "([^"]+)"$"#)]
async fn then_step_result_contains(world: &mut StepWorld, field: String, value: String) {
    let result = world.response_json.as_ref().expect("No step result");
    assert_eq!(
        result[&field].as_str().unwrap_or_default(),
        value,
        "Field '{}' mismatch",
        field,
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// CDC Routing Keys
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[then(regex = r#"^CDC event key for table "([^"]+)" operation "([^"]+)" should be "([^"]+)"$"#)]
async fn then_cdc_event_key(_world: &mut StepWorld, table: String, op: String, expected: String) {
    assert_eq!(routing::event_key(&table, &op), expected);
}

#[then(regex = r#"^CDC queue for deployment "([^"]+)" should be "([^"]+)"$"#)]
async fn then_cdc_queue(_world: &mut StepWorld, hash: String, expected: String) {
    assert_eq!(routing::cdc_queue(&hash), expected);
}

#[then(regex = r#"^CDC wildcard key for table "([^"]+)" should be "([^"]+)"$"#)]
async fn then_cdc_wildcard(_world: &mut StepWorld, table: String, expected: String) {
    assert_eq!(routing::wildcard_key(&table), expected);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// CDC Trigger Config
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[given(regex = r#"^a CDC trigger config for table "([^"]+)" with operations "([^"]+)"$"#)]
async fn given_trigger_config(world: &mut StepWorld, table: String, ops: String) {
    let operations: Vec<CdcOperation> = ops
        .split(',')
        .map(|s| CdcOperation::from_str(s.trim()).expect("Invalid operation"))
        .collect();
    world.cdc_trigger = Some(CdcTriggerConfig {
        cdc_source_id: Uuid::new_v4(),
        pipe_template_id: Uuid::new_v4(),
        table_filter: Some(table),
        operation_filter: Some(operations),
        condition: None,
    });
}

#[given("a minimal CDC trigger config")]
async fn given_minimal_trigger(world: &mut StepWorld) {
    world.cdc_trigger = Some(CdcTriggerConfig {
        cdc_source_id: Uuid::new_v4(),
        pipe_template_id: Uuid::new_v4(),
        table_filter: None,
        operation_filter: None,
        condition: None,
    });
}

#[then(regex = r#"^the trigger should filter table "([^"]+)"$"#)]
async fn then_trigger_filters_table(world: &mut StepWorld, table: String) {
    let trigger = world.cdc_trigger.as_ref().expect("No trigger config");
    assert_eq!(trigger.table_filter.as_deref(), Some(table.as_str()));
}

#[then(regex = r#"^the trigger should filter operations (Insert|Update|Delete) and (Insert|Update|Delete)$"#)]
async fn then_trigger_filters_ops(world: &mut StepWorld, op1: String, op2: String) {
    let trigger = world.cdc_trigger.as_ref().expect("No trigger config");
    let ops = trigger.operation_filter.as_ref().expect("No operation filter");
    let parse = |s: &str| match s {
        "Insert" => CdcOperation::Insert,
        "Update" => CdcOperation::Update,
        "Delete" => CdcOperation::Delete,
        _ => panic!("Unknown op: {}", s),
    };
    assert!(ops.contains(&parse(&op1)), "Missing {}", op1);
    assert!(ops.contains(&parse(&op2)), "Missing {}", op2);
}

#[then("the trigger table filter should be None")]
async fn then_trigger_table_none(world: &mut StepWorld) {
    let trigger = world.cdc_trigger.as_ref().expect("No trigger config");
    assert!(trigger.table_filter.is_none());
}

#[then("the trigger operation filter should be None")]
async fn then_trigger_ops_none(world: &mut StepWorld) {
    let trigger = world.cdc_trigger.as_ref().expect("No trigger config");
    assert!(trigger.operation_filter.is_none());
}

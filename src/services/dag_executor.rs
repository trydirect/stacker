use crate::db;
use crate::models::dag::{DagEdge, DagStep, DagStepExecution};
use crate::services::step_executor;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// DAG Execution Result
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagExecutionResult {
    pub execution_id: Uuid,
    pub status: String,
    pub total_steps: usize,
    pub completed_steps: usize,
    pub failed_steps: usize,
    pub skipped_steps: usize,
    pub execution_order: Vec<Uuid>,
    pub step_results: Vec<StepResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step_id: Uuid,
    pub step_name: String,
    pub step_type: String,
    pub status: String,
    pub output_data: Option<JsonValue>,
    pub error: Option<String>,
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Topological Sort (Kahn's algorithm)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Returns steps grouped by execution level (steps in same level can run in parallel).
pub fn topological_sort(steps: &[DagStep], edges: &[DagEdge]) -> Result<Vec<Vec<Uuid>>, String> {
    if steps.is_empty() {
        return Err("DAG must have at least one step".to_string());
    }

    let step_ids: HashSet<Uuid> = steps.iter().map(|s| s.id).collect();

    // Build adjacency list and in-degree map
    let mut in_degree: HashMap<Uuid, usize> = step_ids.iter().map(|&id| (id, 0)).collect();
    let mut adjacency: HashMap<Uuid, Vec<Uuid>> =
        step_ids.iter().map(|&id| (id, Vec::new())).collect();

    for edge in edges {
        if step_ids.contains(&edge.from_step_id) && step_ids.contains(&edge.to_step_id) {
            adjacency
                .entry(edge.from_step_id)
                .or_default()
                .push(edge.to_step_id);
            *in_degree.entry(edge.to_step_id).or_insert(0) += 1;
        }
    }

    // Kahn's: start with nodes having in-degree 0
    let mut queue: VecDeque<Uuid> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&id, _)| id)
        .collect();

    let mut levels: Vec<Vec<Uuid>> = Vec::new();
    let mut visited_count = 0;

    while !queue.is_empty() {
        let level: Vec<Uuid> = queue.drain(..).collect();
        visited_count += level.len();

        let mut next_queue = VecDeque::new();
        for &node in &level {
            if let Some(neighbors) = adjacency.get(&node) {
                for &neighbor in neighbors {
                    let deg = in_degree.get_mut(&neighbor).unwrap();
                    *deg -= 1;
                    if *deg == 0 {
                        next_queue.push_back(neighbor);
                    }
                }
            }
        }

        levels.push(level);
        queue = next_queue;
    }

    if visited_count != step_ids.len() {
        return Err("DAG contains a cycle".to_string());
    }

    Ok(levels)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Step Executor (delegates to step_executor module)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Execute a single step — delegates to the shared step_executor module.
async fn execute_step(step: &DagStep, input: &JsonValue) -> Result<JsonValue, String> {
    step_executor::execute_step(&step.step_type, &step.config, input).await
}

/// Evaluate a condition — delegates to the shared step_executor module.
#[allow(dead_code)]
fn evaluate_condition(config: &JsonValue, input: &JsonValue) -> bool {
    step_executor::evaluate_condition(config, input)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// DAG Validator
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub fn validate_dag(steps: &[DagStep], _edges: &[DagEdge]) -> Result<(), String> {
    if steps.is_empty() {
        return Err("DAG must have at least one step".to_string());
    }

    let source_types = [
        "source",
        "ws_source",
        "http_stream_source",
        "grpc_source",
        "cdc_source",
        "amqp_source",
        "kafka_source",
    ];
    let target_types = ["target", "ws_target", "grpc_target"];

    let has_source = steps
        .iter()
        .any(|s| source_types.contains(&s.step_type.as_str()));
    if !has_source {
        return Err("DAG must have at least one source step".to_string());
    }

    let has_target = steps
        .iter()
        .any(|s| target_types.contains(&s.step_type.as_str()));
    if !has_target {
        return Err("DAG must have at least one target step".to_string());
    }

    Ok(())
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// DAG Execution Orchestrator
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub async fn execute_dag(
    pool: &PgPool,
    template_id: &Uuid,
    execution_id: Uuid,
    _input_data: &JsonValue,
) -> Result<DagExecutionResult, String> {
    let steps = db::dag::list_steps(pool, template_id).await?;
    let edges = db::dag::list_edges(pool, template_id).await?;

    // Validate
    validate_dag(&steps, &edges)?;

    // Topological sort
    let levels = topological_sort(&steps, &edges)?;

    // Build lookup maps
    let step_map: HashMap<Uuid, &DagStep> = steps.iter().map(|s| (s.id, s)).collect();

    // Build reverse adjacency: for each step, which steps feed into it?
    let mut incoming: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for edge in &edges {
        incoming
            .entry(edge.to_step_id)
            .or_default()
            .push(edge.from_step_id);
    }

    // Build edge condition map (from_step_id → condition) for condition-gated edges
    let mut edge_conditions: HashMap<(Uuid, Uuid), Option<JsonValue>> = HashMap::new();
    for edge in &edges {
        edge_conditions.insert((edge.from_step_id, edge.to_step_id), edge.condition.clone());
    }

    // Create step execution records
    let mut step_exec_ids: HashMap<Uuid, Uuid> = HashMap::new();
    for step in &steps {
        let exec = DagStepExecution::new(execution_id, step.id);
        let saved = db::dag::insert_step_execution(pool, &exec).await?;
        step_exec_ids.insert(step.id, saved.id);
    }

    // Track outputs and statuses
    let mut step_outputs: HashMap<Uuid, JsonValue> = HashMap::new();
    let mut step_statuses: HashMap<Uuid, String> = HashMap::new();
    let mut skipped_steps: HashSet<Uuid> = HashSet::new();
    let mut execution_order: Vec<Uuid> = Vec::new();
    let mut step_results: Vec<StepResult> = Vec::new();

    // Execute level by level
    for level in &levels {
        for &step_id in level {
            let step = step_map[&step_id];
            execution_order.push(step_id);

            // Check if any upstream step failed or was skipped
            let upstream_ids = incoming.get(&step_id).cloned().unwrap_or_default();
            let should_skip = upstream_ids.iter().any(|&up_id| {
                skipped_steps.contains(&up_id)
                    || step_statuses.get(&up_id).map_or(false, |s| s == "failed")
            });

            if should_skip {
                skipped_steps.insert(step_id);
                step_statuses.insert(step_id, "skipped".to_string());

                let exec_id = step_exec_ids[&step_id];
                db::dag::update_step_execution(pool, &exec_id, "skipped", None, None).await?;

                step_results.push(StepResult {
                    step_id,
                    step_name: step.name.clone(),
                    step_type: step.step_type.clone(),
                    status: "skipped".to_string(),
                    output_data: None,
                    error: Some("Upstream step failed or was skipped".to_string()),
                });
                continue;
            }

            // Mark as running
            let exec_id = step_exec_ids[&step_id];
            db::dag::update_step_execution(pool, &exec_id, "running", None, None).await?;

            // Aggregate input from upstream steps
            let input = if upstream_ids.is_empty() {
                serde_json::json!({})
            } else if upstream_ids.len() == 1 {
                step_outputs
                    .get(&upstream_ids[0])
                    .cloned()
                    .unwrap_or(serde_json::json!({}))
            } else {
                // Merge multiple upstream outputs
                let mut merged = serde_json::Map::new();
                for &up_id in &upstream_ids {
                    if let Some(out) = step_outputs.get(&up_id) {
                        if let Some(obj) = out.as_object() {
                            for (k, v) in obj {
                                merged.insert(k.clone(), v.clone());
                            }
                        }
                    }
                }
                JsonValue::Object(merged)
            };

            // Execute the step
            match execute_step(step, &input).await {
                Ok(output) => {
                    // For condition steps, check if condition passed
                    if step.step_type == "condition" {
                        let condition_met = output
                            .get("condition_met")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(true);

                        if !condition_met {
                            // Mark this step as completed but flag downstream for skipping
                            skipped_steps.insert(step_id);
                            step_statuses.insert(step_id, "completed".to_string());
                            step_outputs.insert(step_id, output.clone());

                            db::dag::update_step_execution(
                                pool,
                                &exec_id,
                                "completed",
                                Some(&output),
                                None,
                            )
                            .await?;

                            step_results.push(StepResult {
                                step_id,
                                step_name: step.name.clone(),
                                step_type: step.step_type.clone(),
                                status: "completed".to_string(),
                                output_data: Some(output),
                                error: None,
                            });
                            continue;
                        }
                    }

                    step_statuses.insert(step_id, "completed".to_string());
                    step_outputs.insert(step_id, output.clone());

                    db::dag::update_step_execution(
                        pool,
                        &exec_id,
                        "completed",
                        Some(&output),
                        None,
                    )
                    .await?;

                    step_results.push(StepResult {
                        step_id,
                        step_name: step.name.clone(),
                        step_type: step.step_type.clone(),
                        status: "completed".to_string(),
                        output_data: Some(output),
                        error: None,
                    });
                }
                Err(err) => {
                    step_statuses.insert(step_id, "failed".to_string());

                    db::dag::update_step_execution(pool, &exec_id, "failed", None, Some(&err))
                        .await?;

                    step_results.push(StepResult {
                        step_id,
                        step_name: step.name.clone(),
                        step_type: step.step_type.clone(),
                        status: "failed".to_string(),
                        output_data: None,
                        error: Some(err),
                    });
                }
            }
        }
    }

    // Compute final counts
    let completed_count = step_statuses.values().filter(|s| *s == "completed").count();
    let failed_count = step_statuses.values().filter(|s| *s == "failed").count();
    let skipped_count = step_statuses.values().filter(|s| *s == "skipped").count();

    let overall_status = if failed_count > 0 {
        "partial_failure".to_string()
    } else if skipped_count > 0 && completed_count > 0 {
        "completed".to_string()
    } else {
        "completed".to_string()
    };

    Ok(DagExecutionResult {
        execution_id,
        status: overall_status,
        total_steps: steps.len(),
        completed_steps: completed_count,
        failed_steps: failed_count,
        skipped_steps: skipped_count,
        execution_order,
        step_results,
    })
}

#[cfg(test)]
mod tests {
    use super::{topological_sort, validate_dag};
    use crate::models::dag::{DagEdge, DagStep};
    use serde_json::json;
    use uuid::Uuid;

    fn step(step_type: &str) -> DagStep {
        DagStep::new(Uuid::new_v4(), format!("{step_type}-node"), step_type.to_string(), json!({}))
    }

    fn edge(from: &DagStep, to: &DagStep) -> DagEdge {
        DagEdge::new(from.pipe_template_id, from.id, to.id)
    }

    // ── topological_sort ──────────────────────────────────────────────

    #[test]
    fn topological_sort_rejects_empty_dag() {
        let err = topological_sort(&[], &[]).unwrap_err();
        assert!(err.contains("at least one step"), "{err}");
    }

    #[test]
    fn topological_sort_orders_linear_chain() {
        // a -> b -> c : three sequential levels of one node each.
        let a = step("source");
        let b = step("transform");
        let c = step("target");
        let steps = vec![a.clone(), b.clone(), c.clone()];
        let edges = vec![edge(&a, &b), edge(&b, &c)];

        let levels = topological_sort(&steps, &edges).expect("valid DAG");

        assert_eq!(levels.len(), 3, "linear chain must produce three levels");
        assert_eq!(levels[0], vec![a.id]);
        assert_eq!(levels[1], vec![b.id]);
        assert_eq!(levels[2], vec![c.id]);
    }

    #[test]
    fn topological_sort_groups_parallel_steps_in_same_level() {
        // a -> b, a -> c : b and c are independent and share level 1.
        let a = step("source");
        let b = step("transform");
        let c = step("transform");
        let steps = vec![a.clone(), b.clone(), c.clone()];
        let edges = vec![edge(&a, &b), edge(&a, &c)];

        let levels = topological_sort(&steps, &edges).expect("valid DAG");

        assert_eq!(levels.len(), 2, "should collapse to two levels");
        assert_eq!(levels[0], vec![a.id]);
        let mut parallel = levels[1].clone();
        parallel.sort();
        let mut expected = vec![b.id, c.id];
        expected.sort();
        assert_eq!(parallel, expected, "b and c must run in parallel at level 1");
    }

    #[test]
    fn topological_sort_detects_cycle() {
        // a -> b -> a : no node ever reaches in-degree 0 fully → cycle.
        let a = step("source");
        let b = step("target");
        let steps = vec![a.clone(), b.clone()];
        let edges = vec![edge(&a, &b), edge(&b, &a)];

        let err = topological_sort(&steps, &edges).unwrap_err();
        assert!(err.contains("cycle"), "cycle must be reported: {err}");
    }

    #[test]
    fn topological_sort_detects_self_loop() {
        let a = step("source");
        let steps = vec![a.clone()];
        let edges = vec![edge(&a, &a)];

        let err = topological_sort(&steps, &edges).unwrap_err();
        assert!(err.contains("cycle"), "self-loop is a cycle: {err}");
    }

    #[test]
    fn topological_sort_ignores_edges_to_unknown_steps() {
        // An edge referencing a step id that isn't in `steps` must be skipped,
        // not counted as a dependency (otherwise a valid DAG would appear cyclic
        // or leave a node permanently blocked).
        let a = step("source");
        let b = step("target");
        let dangling = step("transform"); // not included in steps
        let steps = vec![a.clone(), b.clone()];
        let edges = vec![edge(&a, &b), edge(&dangling, &b)];

        let levels = topological_sort(&steps, &edges).expect("dangling edge should be ignored");
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[0], vec![a.id]);
        assert_eq!(levels[1], vec![b.id]);
    }

    #[test]
    fn topological_sort_handles_disconnected_nodes_in_first_level() {
        // Two independent nodes with no edges both start at level 0.
        let a = step("source");
        let b = step("target");
        let steps = vec![a.clone(), b.clone()];

        let levels = topological_sort(&steps, &[]).expect("no edges is valid");
        assert_eq!(levels.len(), 1, "all roots share the first level");
        let mut first = levels[0].clone();
        first.sort();
        let mut expected = vec![a.id, b.id];
        expected.sort();
        assert_eq!(first, expected);
    }

    // ── validate_dag ──────────────────────────────────────────────────

    #[test]
    fn validate_dag_rejects_empty() {
        let err = validate_dag(&[], &[]).unwrap_err();
        assert!(err.contains("at least one step"), "{err}");
    }

    #[test]
    fn validate_dag_requires_a_source() {
        // transform + target but no source.
        let steps = vec![step("transform"), step("target")];
        let err = validate_dag(&steps, &[]).unwrap_err();
        assert!(err.contains("source step"), "{err}");
    }

    #[test]
    fn validate_dag_requires_a_target() {
        let steps = vec![step("source"), step("transform")];
        let err = validate_dag(&steps, &[]).unwrap_err();
        assert!(err.contains("target step"), "{err}");
    }

    #[test]
    fn validate_dag_accepts_source_and_target() {
        let steps = vec![step("source"), step("target")];
        assert!(validate_dag(&steps, &[]).is_ok());
    }

    #[test]
    fn validate_dag_accepts_alternate_source_and_target_types() {
        // ws_source / grpc_target are in the recognised source/target sets.
        let steps = vec![step("ws_source"), step("grpc_target")];
        assert!(validate_dag(&steps, &[]).is_ok());
    }
}

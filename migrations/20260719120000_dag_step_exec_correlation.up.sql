-- Correlation key for the AMQP agent-executor path.
-- StepResultMsg carries (execution_id, step_id); the platform must map that
-- back to exactly one pipe_dag_step_executions row to persist the result.
-- A unique index guarantees the mapping is unambiguous and lets us look rows
-- up by (pipe_execution_id, step_id) instead of the surrogate row id.
CREATE UNIQUE INDEX idx_dag_step_exec_lookup
    ON pipe_dag_step_executions (pipe_execution_id, step_id);

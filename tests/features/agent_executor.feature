Feature: Agent Executor Protocol
  As a pipe orchestrator
  I want an agent-executor that processes step commands via AMQP protocol
  So that step execution is decoupled from the main server

  Background:
    Given I am authenticated as User A

  # --- Protocol Serialization ---

  Scenario: StepCommand serializes and deserializes correctly
    Given a StepCommand with step_type "source" and config:
      """
      {"url": "https://api.example.com/data", "output": {"rows": 10}}
      """
    When the command is serialized to JSON and back
    Then the deserialized command should match the original

  Scenario: StepResultMsg success serializes correctly
    Given a successful StepResultMsg with output:
      """
      {"rows": 42, "status": "ok"}
      """
    When the result is serialized to JSON and back
    Then the deserialized result status should be "completed"
    And the deserialized result should have output data

  Scenario: StepResultMsg failure serializes correctly
    Given a failed StepResultMsg with error "connection refused"
    When the result is serialized to JSON and back
    Then the deserialized result status should be "failed"
    And the deserialized result error should be "connection refused"

  # --- Step Execution via Agent Protocol ---

  Scenario: Execute source step via agent protocol
    Given a StepCommand with step_type "source" and config:
      """
      {"output": {"sensor": "temperature", "value": 23.5}}
      """
    When the step is executed via step_executor
    Then the execution should succeed
    And the output should contain key "sensor" with value "temperature"

  Scenario: Execute transform step via agent protocol
    Given a StepCommand with step_type "transform" and config:
      """
      {"mapping": {"name": true, "email": true}}
      """
    And the step input is:
      """
      {"name": "Alice", "email": "alice@example.com", "age": 30}
      """
    When the step is executed via step_executor
    Then the execution should succeed
    And the output should contain key "name" with value "Alice"

  Scenario: Execute condition step via agent protocol
    Given a StepCommand with step_type "condition" and config:
      """
      {"field": "score", "operator": "gt", "value": 50}
      """
    And the step input is:
      """
      {"score": 75}
      """
    When the step is executed via step_executor
    Then the execution should succeed
    And the output should contain key "condition_met" with value "true"

  Scenario: Unknown step type returns error
    Given a StepCommand with step_type "nonexistent" and config:
      """
      {}
      """
    When the step is executed via step_executor
    Then the execution should fail with error containing "Unknown step type"

  # --- Circuit Breaker ---

  Scenario: Circuit breaker blocks after threshold failures
    Given a circuit breaker with failure_threshold 3 and recovery_timeout 60
    When I record 3 consecutive failures
    Then the circuit breaker should be in "open" state
    And the circuit breaker should reject requests

  Scenario: Circuit breaker recovers after timeout
    Given a circuit breaker with failure_threshold 2 and recovery_timeout 1
    When I record 2 consecutive failures
    Then the circuit breaker should be in "open" state
    When I wait 2 seconds for recovery
    Then the circuit breaker should be in "halfopen" state
    And the circuit breaker should allow requests

  # --- Retry + Resilience ---

  Scenario: Retry exhaustion returns final error
    Given a StepCommand with step_type "source" and config:
      """
      {"error": "simulated failure"}
      """
    And a retry policy with max_retries 2 and backoff_base_ms 10
    When the step is executed with resilience
    Then the execution should fail with error containing "simulated failure"

  Scenario: Routing key generation
    Given a deployment hash "deploy-abc-123"
    Then the execute routing key should be "pipe.step.execute.deploy-abc-123"
    And the result routing key should be "pipe.step.result.deploy-abc-123"
    And the agent queue name should be "agent_executor_deploy-abc-123"

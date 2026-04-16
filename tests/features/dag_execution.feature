Feature: DAG Execution Engine
  As a pipe owner
  I want to execute a DAG pipeline with multiple steps
  So that data flows through source → transform → target in topological order

  Background:
    Given I am authenticated as User A
    And I have a DAG pipe template named "exec-dag"
    And I have a DAG pipe instance for that template

  # ── Topological Sort & Execution ────────────────────────────

  Scenario: Execute a simple linear DAG (Source → Transform → Target)
    Given I have added a DAG step "Source" of type "source" with config:
      """
      {"output": {"message": "hello"}}
      """
    And I have added a DAG step "Transform" of type "transform" with config:
      """
      {"mapping": {"msg": "$.message"}}
      """
    And I have added a DAG step "Target" of type "target" with config:
      """
      {"destination": "output"}
      """
    And I have added a DAG edge from step "Source" to step "Transform"
    And I have added a DAG edge from step "Transform" to step "Target"
    When I execute the DAG with input:
      """
      {"trigger": "manual"}
      """
    Then the response status should be 200
    And the response JSON at "/item/status" should be "completed"
    And the response JSON at "/item/total_steps" should be "3"
    And the response JSON at "/item/completed_steps" should be "3"
    And the response JSON at "/item/failed_steps" should be "0"

  Scenario: Execution creates step-level records for each DAG step
    Given I have added a DAG step "Source" of type "source" with config:
      """
      {"output": {"data": 1}}
      """
    And I have added a DAG step "Target" of type "target" with config:
      """
      {"destination": "sink"}
      """
    And I have added a DAG edge from step "Source" to step "Target"
    When I execute the DAG with input:
      """
      {}
      """
    Then the response status should be 200
    When I list step executions for the DAG execution
    Then the response status should be 200
    And the response JSON at "/list" should have length 2
    And every step execution should have status "completed"

  Scenario: Topological ordering respects edge dependencies
    Given I have added a DAG step "Source" of type "source" with config:
      """
      {"output": {"val": "start"}}
      """
    And I have added a DAG step "Mid1" of type "transform" with config:
      """
      {"mapping": {"x": "$.val"}}
      """
    And I have added a DAG step "Mid2" of type "transform" with config:
      """
      {"mapping": {"y": "$.x"}}
      """
    And I have added a DAG step "Target" of type "target" with config:
      """
      {"destination": "end"}
      """
    And I have added a DAG edge from step "Source" to step "Mid1"
    And I have added a DAG edge from step "Mid1" to step "Mid2"
    And I have added a DAG edge from step "Mid2" to step "Target"
    When I execute the DAG with input:
      """
      {}
      """
    Then the response status should be 200
    And the response JSON at "/item/status" should be "completed"
    And the response JSON at "/item/total_steps" should be "4"
    And the response JSON at "/item/completed_steps" should be "4"

  # ── Condition Steps ─────────────────────────────────────────

  Scenario: Condition step evaluates to true and continues
    Given I have added a DAG step "Source" of type "source" with config:
      """
      {"output": {"score": 85}}
      """
    And I have added a DAG step "Check" of type "condition" with config:
      """
      {"expression": "$.score > 50", "field": "score", "operator": "gt", "value": 50}
      """
    And I have added a DAG step "Target" of type "target" with config:
      """
      {"destination": "pass"}
      """
    And I have added a DAG edge from step "Source" to step "Check"
    And I have added a DAG edge from step "Check" to step "Target"
    When I execute the DAG with input:
      """
      {}
      """
    Then the response status should be 200
    And the response JSON at "/item/status" should be "completed"
    And the response JSON at "/item/completed_steps" should be "3"
    And the response JSON at "/item/skipped_steps" should be "0"

  Scenario: Condition step evaluates to false and skips downstream
    Given I have added a DAG step "Source" of type "source" with config:
      """
      {"output": {"score": 30}}
      """
    And I have added a DAG step "Check" of type "condition" with config:
      """
      {"field": "score", "operator": "gt", "value": 50}
      """
    And I have added a DAG step "Target" of type "target" with config:
      """
      {"destination": "pass"}
      """
    And I have added a DAG edge from step "Source" to step "Check"
    And I have added a DAG edge from step "Check" to step "Target"
    When I execute the DAG with input:
      """
      {}
      """
    Then the response status should be 200
    And the response JSON at "/item/status" should be "completed"
    And the response JSON at "/item/skipped_steps" should be "1"

  # ── Parallel Execution ──────────────────────────────────────

  Scenario: Parallel branches execute independently
    Given I have added a DAG step "Source" of type "source" with config:
      """
      {"output": {"data": "input"}}
      """
    And I have added a DAG step "BranchA" of type "transform" with config:
      """
      {"mapping": {"a": "$.data"}}
      """
    And I have added a DAG step "BranchB" of type "transform" with config:
      """
      {"mapping": {"b": "$.data"}}
      """
    And I have added a DAG step "Target" of type "target" with config:
      """
      {"destination": "merged"}
      """
    And I have added a DAG edge from step "Source" to step "BranchA"
    And I have added a DAG edge from step "Source" to step "BranchB"
    And I have added a DAG edge from step "BranchA" to step "Target"
    And I have added a DAG edge from step "BranchB" to step "Target"
    When I execute the DAG with input:
      """
      {}
      """
    Then the response status should be 200
    And the response JSON at "/item/status" should be "completed"
    And the response JSON at "/item/total_steps" should be "4"
    And the response JSON at "/item/completed_steps" should be "4"

  # ── Failure Handling ────────────────────────────────────────

  Scenario: Step failure marks execution as partial_failure
    Given I have added a DAG step "Source" of type "source" with config:
      """
      {"output": {"x": 1}}
      """
    And I have added a DAG step "BadStep" of type "transform" with config:
      """
      {"error": "simulated failure"}
      """
    And I have added a DAG step "Target" of type "target" with config:
      """
      {"destination": "out"}
      """
    And I have added a DAG edge from step "Source" to step "BadStep"
    And I have added a DAG edge from step "BadStep" to step "Target"
    When I execute the DAG with input:
      """
      {}
      """
    Then the response status should be 200
    And the response JSON at "/item/status" should be "partial_failure"
    And the response JSON at "/item/failed_steps" should be "1"
    And the response JSON at "/item/skipped_steps" should be "1"

  Scenario: Downstream steps are skipped when upstream fails
    Given I have added a DAG step "Source" of type "source" with config:
      """
      {"output": {"x": 1}}
      """
    And I have added a DAG step "Fail" of type "transform" with config:
      """
      {"error": "broken"}
      """
    And I have added a DAG step "After" of type "transform" with config:
      """
      {"mapping": {"y": "$.x"}}
      """
    And I have added a DAG step "Target" of type "target" with config:
      """
      {"destination": "end"}
      """
    And I have added a DAG edge from step "Source" to step "Fail"
    And I have added a DAG edge from step "Fail" to step "After"
    And I have added a DAG edge from step "After" to step "Target"
    When I execute the DAG with input:
      """
      {}
      """
    Then the response status should be 200
    And the response JSON at "/item/failed_steps" should be "1"
    And the response JSON at "/item/skipped_steps" should be "2"

  # ── Validation Failures ─────────────────────────────────────

  Scenario: Execute fails on invalid DAG (no steps)
    When I execute the DAG with input:
      """
      {}
      """
    Then the response status should be 400
    And the response body should contain "at least one step"

  Scenario: Execute fails on DAG missing source step
    Given I have added a DAG step "OnlyTarget" of type "target" with config:
      """
      {"destination": "out"}
      """
    When I execute the DAG with input:
      """
      {}
      """
    Then the response status should be 400
    And the response body should contain "source step"

  # ── Cross-user Isolation ────────────────────────────────────

  Scenario: Another user cannot execute my DAG
    Given I have added DAG steps "Source, Target" to the template
    And I have added a DAG edge from step "Source" to step "Target"
    When another user executes the DAG for the stored template
    Then the response status should be 404

  # ── Step Execution Status Endpoint ──────────────────────────

  Scenario: List step executions returns per-step details
    Given I have added a DAG step "Source" of type "source" with config:
      """
      {"output": {"k": "v"}}
      """
    And I have added a DAG step "Target" of type "target" with config:
      """
      {"destination": "out"}
      """
    And I have added a DAG edge from step "Source" to step "Target"
    When I execute the DAG with input:
      """
      {}
      """
    Then the response status should be 200
    When I list step executions for the DAG execution
    Then the response status should be 200
    And the response JSON at "/list" should have length 2
    And each step execution should have a "step_id" field
    And each step execution should have a "status" field

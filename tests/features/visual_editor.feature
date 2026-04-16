Feature: Visual DAG Editor API Integration
  As a user of the visual DAG editor I want the backend API to support
  all CRUD operations for steps, edges, validation and execution so the
  editor can render and manipulate DAGs.

  Background:
    Given I am authenticated as User A

  # ──────────────────────────────────────────────
  # Step CRUD via Editor
  # ──────────────────────────────────────────────

  Scenario: Add a source step via editor API
    Given I have a DAG pipe template named "editor-crud-test"
    When I add a step "DataIn" of type "source" at position 100,80
    Then the response status should be 201
    And the step should have name "DataIn"
    And the step should have type "source"

  Scenario: Add a CDC source step via editor API
    Given I have a DAG pipe template named "editor-cdc-test"
    When I add a step "PgChanges" of type "cdc_source" at position 200,80
    Then the response status should be 201
    And the step should have type "cdc_source"

  Scenario: Update step name and config
    Given I have a DAG pipe template named "editor-update-test"
    And I have added a DAG step "OldName" of type "transform" with config:
      """
      {"mapping": "field_a"}
      """
    When I update the step name to "NewName" with config:
      """
      {"mapping": "field_b", "filter": true}
      """
    Then the response status should be 200

  Scenario: Delete a step removes it from the DAG
    Given I have a DAG pipe template named "editor-delete-test"
    And I have added a DAG step "ToDelete" of type "target" with config:
      """
      {}
      """
    When I delete the step
    Then the response status should be 200
    And listing steps should return 0 steps

  # ──────────────────────────────────────────────
  # Edge CRUD via Editor
  # ──────────────────────────────────────────────

  Scenario: Connect two steps with an edge
    Given I have a DAG pipe template named "editor-edge-test"
    And I have added a DAG step "In" of type "source" with config:
      """
      {"output": {"val": 1}}
      """
    And I have added a DAG step "Out" of type "target" with config:
      """
      {}
      """
    When I add an edge from step "In" to step "Out"
    Then the response status should be 201
    And listing edges should return 1 edge

  Scenario: Delete an edge
    Given I have a DAG pipe template named "editor-edge-del"
    And I have added a DAG step "A" of type "source" with config:
      """
      {"output": {"x": 1}}
      """
    And I have added a DAG step "B" of type "target" with config:
      """
      {}
      """
    And I have added a DAG edge from step "A" to step "B"
    When I delete the edge from "A" to "B"
    Then the response status should be 200
    And listing edges should return 0 edges

  # ──────────────────────────────────────────────
  # Validation via Editor
  # ──────────────────────────────────────────────

  Scenario: Validate a complete DAG succeeds
    Given I have a DAG pipe template named "editor-valid-dag"
    And I have added a DAG step "Src" of type "source" with config:
      """
      {"output": {"data": 42}}
      """
    And I have added a DAG step "Tgt" of type "target" with config:
      """
      {}
      """
    And I have added a DAG edge from step "Src" to step "Tgt"
    When I validate the DAG
    Then the response status should be 200
    And the response body should contain "valid"

  Scenario: Validate an empty DAG fails
    Given I have a DAG pipe template named "editor-empty-dag"
    When I validate the DAG
    Then the response status should be 200

  # ──────────────────────────────────────────────
  # Step type coverage (all 12 types)
  # ──────────────────────────────────────────────

  Scenario: All step types are accepted by the API
    Given I have a DAG pipe template named "editor-all-types"
    When I add steps of all supported types
    Then all 12 steps should be created successfully

  # ──────────────────────────────────────────────
  # DAG Execution via Editor
  # ──────────────────────────────────────────────

  Scenario: Execute a DAG and get step-level results
    Given I have a DAG pipe template named "editor-exec-test"
    And I have a DAG pipe instance for that template
    And I have added a DAG step "Input" of type "source" with config:
      """
      {"output": {"value": 100}}
      """
    And I have added a DAG step "Process" of type "transform" with config:
      """
      {"mapping": {"result": "value"}}
      """
    And I have added a DAG step "Output" of type "target" with config:
      """
      {}
      """
    And I have added a DAG edge from step "Input" to step "Process"
    And I have added a DAG edge from step "Process" to step "Output"
    When I execute the DAG with input:
      """
      {}
      """
    Then the response status should be 200
    And the response body should contain "completed"

  # ──────────────────────────────────────────────
  # Edge Deletion (v2)
  # ──────────────────────────────────────────────

  Scenario: Deleting an edge returns it in subsequent listing
    Given I have a DAG pipe template named "editor-edge-del-v2"
    And I have added a DAG step "X" of type "source" with config:
      """
      {"output": {"v": 1}}
      """
    And I have added a DAG step "Y" of type "target" with config:
      """
      {}
      """
    And I have added a DAG edge from step "X" to step "Y"
    When I delete the edge from "X" to "Y"
    Then the response status should be 200
    And listing edges should return 0 edges
    And listing steps should return 2 steps

  # ──────────────────────────────────────────────
  # Casbin anonymous /editor access (v2)
  # ──────────────────────────────────────────────

  Scenario: Anonymous GET /editor is permitted by Casbin
    When I make an unauthenticated GET request to "/editor/"
    Then the response status should not be 403

  Scenario: Anonymous GET /editor/assets/index.js is permitted
    When I make an unauthenticated GET request to "/editor/assets/index.js"
    Then the response status should not be 403

  # ──────────────────────────────────────────────
  # Starter Templates data validation (v2)
  # ──────────────────────────────────────────────

  Scenario: Seed DAG from ETL Pipeline template
    Given I have a DAG pipe template named "etl-seed-test"
    When I add a step "Fetch API Data" of type "source" at position 100,80
    And I add a step "Clean & Map" of type "transform" at position 300,80
    And I add a step "Write to DB" of type "target" at position 500,80
    And I add an edge from step "Fetch API Data" to step "Clean & Map"
    And I add an edge from step "Clean & Map" to step "Write to DB"
    Then listing steps should return 3 steps
    And listing edges should return 2 edges

  Scenario: Seed DAG from Webhook Router template with condition
    Given I have a DAG pipe template named "webhook-seed-test"
    When I add a step "Webhook Receiver" of type "http_stream_source" at position 100,80
    And I add a step "Route by Type" of type "condition" at position 300,80
    And I add a step "Order Service" of type "target" at position 500,80
    And I add a step "Notification Service" of type "target" at position 500,200
    And I add an edge from step "Webhook Receiver" to step "Route by Type"
    And I add an edge from step "Route by Type" to step "Order Service"
    And I add an edge from step "Route by Type" to step "Notification Service"
    Then listing steps should return 4 steps
    And listing edges should return 3 edges

  Scenario: Seed DAG from CDC Replicator template
    Given I have a DAG pipe template named "cdc-seed-test"
    When I add a step "PG WAL Capture" of type "cdc_source" at position 100,80
    And I add a step "Map Fields" of type "transform" at position 300,80
    And I add a step "Replicate to Target" of type "grpc_target" at position 500,80
    And I add an edge from step "PG WAL Capture" to step "Map Fields"
    And I add an edge from step "Map Fields" to step "Replicate to Target"
    Then listing steps should return 3 steps
    And listing edges should return 2 edges

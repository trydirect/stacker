Feature: CDC PostgreSQL Change Data Capture
  As a user I want to capture database changes (INSERT/UPDATE/DELETE) from PostgreSQL
  and route them through pipe DAGs for real-time data integration.

  Background:
    Given I am authenticated as User A

  # ──────────────────────────────────────────────
  # CDC Event Model
  # ──────────────────────────────────────────────

  Scenario: CDC change event INSERT has correct row data
    Given a CDC change event for table "users" with operation "INSERT"
    And the event has after data:
      """
      {"id": 1, "name": "Alice", "email": "alice@test.com"}
      """
    Then the event row_data should match the after data
    And the event before data should be null

  Scenario: CDC change event UPDATE has both before and after
    Given a CDC change event for table "orders" with operation "UPDATE"
    And the event has before data:
      """
      {"id": 1, "total": 100.0, "status": "pending"}
      """
    And the event has after data:
      """
      {"id": 1, "total": 150.0, "status": "confirmed"}
      """
    Then the event row_data should match the after data
    And the event before data should not be null

  Scenario: CDC change event DELETE has only before data
    Given a CDC change event for table "sessions" with operation "DELETE"
    And the event has before data:
      """
      {"id": 42, "user_id": 1, "token": "abc123"}
      """
    Then the event row_data should match the before data
    And the event after data should be null

  # ──────────────────────────────────────────────
  # CDC Operation Parsing
  # ──────────────────────────────────────────────

  Scenario: Parse CDC operations from standard names
    Then CDC operation "INSERT" should parse to Insert
    And CDC operation "UPDATE" should parse to Update
    And CDC operation "DELETE" should parse to Delete

  Scenario: Parse CDC operations from short codes
    Then CDC operation "I" should parse to Insert
    And CDC operation "U" should parse to Update
    And CDC operation "D" should parse to Delete

  Scenario: Invalid CDC operation returns None
    Then CDC operation "TRUNCATE" should not parse
    And CDC operation "bogus" should not parse

  # ──────────────────────────────────────────────
  # CDC Pipe Payload
  # ──────────────────────────────────────────────

  Scenario: CDC event produces valid pipe payload
    Given a CDC change event for table "products" with operation "INSERT"
    And the event has after data:
      """
      {"id": 5, "name": "Widget", "price": 29.99}
      """
    When I convert the event to a pipe payload
    Then the payload should contain field "table" with value "products"
    And the payload should contain field "operation" with value "INSERT"
    And the payload should contain field "schema" with value "public"
    And the payload "after" should have key "name"

  # ──────────────────────────────────────────────
  # CDC Source Step Execution
  # ──────────────────────────────────────────────

  Scenario: Execute cdc_source step with config
    Given a DAG step of type "cdc_source" with config:
      """
      {"replication_slot": "test_slot", "publication": "test_pub", "tables": ["users", "orders"]}
      """
    When I execute the step with empty input
    Then the step result should contain "cdc_connected" as true
    And the step result should contain "replication_slot" as "test_slot"
    And the step result should contain "status" as "listening"

  Scenario: Execute cdc_source step with explicit output
    Given a DAG step of type "cdc_source" with config:
      """
      {"output": {"event": "insert", "table": "users", "data": {"id": 1}}}
      """
    When I execute the step with empty input
    Then the step result should contain "event" as "insert"
    And the step result should contain "table" as "users"

  # ──────────────────────────────────────────────
  # CDC Routing Keys
  # ──────────────────────────────────────────────

  Scenario: CDC routing keys follow convention
    Then CDC event key for table "users" operation "INSERT" should be "cdc.event.users.insert"
    And CDC event key for table "orders" operation "DELETE" should be "cdc.event.orders.delete"
    And CDC queue for deployment "deploy-abc" should be "cdc_listener_deploy-abc"
    And CDC wildcard key for table "users" should be "cdc.event.users.#"

  # ──────────────────────────────────────────────
  # CDC Trigger Config
  # ──────────────────────────────────────────────

  Scenario: CDC trigger config with filters
    Given a CDC trigger config for table "users" with operations "INSERT,UPDATE"
    Then the trigger should filter table "users"
    And the trigger should filter operations Insert and Update

  Scenario: CDC trigger config minimal (no filters)
    Given a minimal CDC trigger config
    Then the trigger table filter should be None
    And the trigger operation filter should be None

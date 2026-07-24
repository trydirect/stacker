Feature: Resilience — Circuit Breaker, Dead Letter Queue, and Retry
  As a user I want failed pipe executions to be captured in a DLQ
  and circuit breakers to protect against cascading failures

  Background:
    Given I am authenticated as User A
    And I have a resilience pipe template named "bdd-resilience-tpl"
    And I have a resilience pipe instance for that template

  # --- Dead Letter Queue ---

  Scenario: DLQ is initially empty for a pipe instance
    When I list DLQ entries for the pipe instance
    Then the response status should be 200
    And the response JSON at "/list" should be an empty array

  Scenario: Create a DLQ entry for a failed execution
    Given I have a failed pipe execution for the instance
    When I push the failed execution to the DLQ with:
      """
      {"max_retries": 3, "error": "Connection refused"}
      """
    Then the response status should be 201
    And the response JSON at "/item/status" should be "pending"
    And the response JSON at "/item/retry_count" should be "0"

  Scenario: List DLQ entries for a pipe instance
    Given I have a DLQ entry for the pipe instance
    When I list DLQ entries for the pipe instance
    Then the response status should be 200
    And the response JSON at "/list" should not be empty

  Scenario: Get a single DLQ entry
    Given I have a DLQ entry for the pipe instance
    When I get the stored DLQ entry
    Then the response status should be 200
    And the response JSON at "/item" should not be empty
    And the response JSON at "/item/retry_count" should be "0"

  Scenario: Retry a DLQ entry increments retry count
    Given I have a DLQ entry for the pipe instance
    When I retry the stored DLQ entry
    Then the response status should be 200
    And the response JSON at "/item/retry_count" should be "1"
    And the response JSON at "/item/status" should be "retrying"

  Scenario: Discard a DLQ entry
    Given I have a DLQ entry for the pipe instance
    When I discard the stored DLQ entry
    Then the response status should be one of "200, 204"

  Scenario: Discarded DLQ entry no longer appears in list
    Given I have a DLQ entry for the pipe instance
    And I discard the stored DLQ entry
    When I list DLQ entries for the pipe instance
    Then the response status should be 200
    And the response JSON at "/list" should be an empty array

  Scenario: DLQ entry moves to exhausted after max retries
    Given I have a DLQ entry with max_retries 1 for the pipe instance
    When I retry the stored DLQ entry
    Then the response status should be 200
    And the response JSON at "/item/status" should be "exhausted"

  # --- Circuit Breaker ---

  Scenario: Circuit breaker defaults to closed state
    When I get the circuit breaker status for the pipe instance
    Then the response status should be 200
    And the response JSON at "/item/state" should be "closed"
    And the response JSON at "/item/failure_count" should be "0"

  Scenario: Update circuit breaker config
    When I update the circuit breaker config for the pipe instance with:
      """
      {"failure_threshold": 5, "recovery_timeout_seconds": 30, "half_open_max_requests": 2}
      """
    Then the response status should be 200
    And the response JSON at "/item/failure_threshold" should be "5"

  Scenario: Record a failure increments failure count
    Given the circuit breaker is configured with failure_threshold 3 for the pipe instance
    When I record a circuit breaker failure for the pipe instance
    Then the response status should be 200
    And the response JSON at "/item/failure_count" should be "1"
    And the response JSON at "/item/state" should be "closed"

  Scenario: Circuit breaker opens after threshold failures
    Given the circuit breaker is configured with failure_threshold 2 for the pipe instance
    When I record a circuit breaker failure for the pipe instance
    And I record a circuit breaker failure for the pipe instance
    Then the response status should be 200
    And the response JSON at "/item/state" should be "open"

  Scenario: Reset circuit breaker returns to closed
    Given the circuit breaker is in open state for the pipe instance
    When I reset the circuit breaker for the pipe instance
    Then the response status should be 200
    And the response JSON at "/item/state" should be "closed"
    And the response JSON at "/item/failure_count" should be "0"

  Scenario: Record a success resets failure count in closed state
    Given the circuit breaker is configured with failure_threshold 5 for the pipe instance
    And I have recorded 2 circuit breaker failures for the pipe instance
    When I record a circuit breaker success for the pipe instance
    Then the response status should be 200
    And the response JSON at "/item/failure_count" should be "0"

  # --- Cross-user isolation ---

  Scenario: Cross-user cannot access another user's DLQ
    Given I have a DLQ entry for the pipe instance
    When I switch to User B
    And I list DLQ entries for the pipe instance
    Then the response status should be one of "403, 404"

  Scenario: Cross-user cannot access another user's circuit breaker
    When I switch to User B
    And I get the circuit breaker status for the pipe instance
    Then the response status should be one of "403, 404"

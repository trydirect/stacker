Feature: Pipe Executions
  Users can list execution history for pipe instances,
  view execution details, and replay past executions.

  Background:
    Given I am authenticated as User A
    And I have a test deployment with hash "bdd-deployment-exec"

  Scenario: List executions for a new pipe instance (empty)
    Given I have created a pipe instance for deployment "bdd-deployment-exec" with source "exec-src" and target container "exec-tgt"
    When I list executions for the stored pipe instance
    Then the response status should be 200
    And the response JSON should have key "list"

  Scenario: Get execution not found
    Given I am authenticated as User A
    When I get pipe execution "00000000-0000-0000-0000-000000000000"
    Then the response status should be 404

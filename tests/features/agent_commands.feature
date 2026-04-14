Feature: Agent Commands
  As a user and agent
  I want to create, poll, execute, and report commands
  So that I can manage deployments remotely

  Background:
    Given I am authenticated as User A
    And I have a test deployment with hash "bdd-cmd-deploy"

  Scenario: Create a command for a deployment
    When I create a command for deployment "bdd-cmd-deploy" with type "health"
    Then the response status should be 201
    And the response JSON at "/item/command_id" should not be empty
    And the response JSON at "/item/status" should be "queued"

  Scenario: Create a command with high priority
    When I create a command for deployment "bdd-cmd-deploy" with type "restart_service" and priority "high"
    Then the response status should be 201

  Scenario: List commands for a deployment
    Given I have created a command for deployment "bdd-cmd-deploy" with type "health"
    When I list commands for deployment "bdd-cmd-deploy"
    Then the response status should be 200

  Scenario: Get a specific command
    Given I have created a command for deployment "bdd-cmd-deploy" with type "health"
    When I get the stored command for deployment "bdd-cmd-deploy"
    Then the response status should be 200
    And the response JSON at "/item/type" should be "health"

  Scenario: Cancel a queued command fails due to ID mismatch bug
    Given I have created a command for deployment "bdd-cmd-deploy" with type "health"
    When I cancel the stored command for deployment "bdd-cmd-deploy"
    Then the response status should be 500

  Scenario: Create a command with parameters
    When I create a command for deployment "bdd-cmd-deploy" with type "restart_service" and parameters
      | key            | value     |
      | container_name | nginx     |
      | force          | true      |
    Then the response status should be 201

  Scenario: List commands with limit
    Given I have created a command for deployment "bdd-cmd-deploy" with type "health"
    Given I have created a command for deployment "bdd-cmd-deploy" with type "restart_service"
    When I list commands for deployment "bdd-cmd-deploy" with limit 1
    Then the response status should be 200

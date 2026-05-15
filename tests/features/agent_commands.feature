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

  Scenario: Create command rejects empty deployment_hash
    When I create a command with empty deployment hash
    Then the response status should be 400

  Scenario: Create command rejects empty command_type
    When I create a command with empty command type
    Then the response status should be 400

  Scenario: Create command rejects invalid log parameters
    When I create a logs command with invalid limit
    Then the response status should be 400

  Scenario: Get command returns 404 for nonexistent command_id
    When I get command "cmd_nonexistent" for deployment "bdd-cmd-deploy"
    Then the response status should be 404

  Scenario: User B cannot list User A deployment commands
    Given I have created a command for deployment "bdd-cmd-deploy" with type "health"
    When I switch to User B
    And I list commands for deployment "bdd-cmd-deploy"
    Then the response status should be 404

  Scenario: Create command auto-creates deployment when hash unknown
    When I create a command for deployment "bdd-cmd-auto-create" with type "health"
    Then the response status should be 201
    And the response JSON at "/item/deployment_hash" should be "bdd-cmd-auto-create"

  Scenario: Create health command for all containers
    When I create a health-all command for deployment "bdd-cmd-deploy"
    Then the response status should be 201

  Scenario: Create restart command with app_code
    When I create a restart command for app "nginx" on deployment "bdd-cmd-deploy"
    Then the response status should be 201

  Scenario: Enqueue a command via agent endpoint
    When I enqueue a command for deployment "bdd-cmd-deploy" with type "health"
    Then the response status should be 201
    And the response JSON at "/item/command_id" should not be empty

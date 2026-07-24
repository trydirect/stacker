Feature: Agent Snapshots
  As a monitoring system
  I want to get deployment snapshots
  So that I can display agent and container status

  Scenario: Get snapshot for a deployment with no agent
    Given I am authenticated as User A
    And I have a test deployment with hash "bdd-snap-deploy"
    When I get the snapshot for deployment "bdd-snap-deploy"
    Then the response status should be 200

  Scenario: Get snapshot for nonexistent deployment
    When I get the snapshot for deployment "nonexistent-hash-999"
    Then the response status should be 200

  Scenario: Get project snapshot with no active agent
    Given I am authenticated as User A
    And I have a test deployment with hash "bdd-snap-proj"
    When I get the project snapshot for the stored project
    Then the response status should be 200

Feature: Deployments
  As a user managing cloud deployments
  I want to view and manage deployment status
  So that I can monitor my infrastructure

  Background:
    Given I am authenticated as User A
    And I have a test deployment with hash "bdd-deploy-main"

  Scenario: List deployments
    When I list deployments
    Then the response status should be 200

  Scenario: Get deployment by ID
    When I get the stored deployment by ID
    Then the response status should be 200

  Scenario: Get deployment by hash
    When I get the deployment by hash "bdd-deploy-main"
    Then the response status should be 200

  Scenario: Get deployment by project
    When I get the deployment by project
    Then the response status should be one of "200, 404"

  Scenario: Force complete an error deployment
    Given I have a test deployment with hash "bdd-deploy-err" and status "error"
    When I force complete deployment "bdd-deploy-err"
    Then the response status should be 200

  Scenario: Force complete a running deployment is rejected
    When I force complete the stored deployment
    Then the response status should be 400

  Scenario: Get capabilities for a deployment
    When I get capabilities for deployment "bdd-deploy-main"
    Then the response status should be 200

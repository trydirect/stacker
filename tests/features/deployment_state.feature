Feature: Deployment state
  As an AI-aware operator
  I want a canonical deployment state endpoint
  So that I can inspect current deployment reality without stitching commands

  Background:
    Given I am authenticated as User A
    And I have a test deployment with hash "bdd-deploy-state"

  Scenario: Get canonical deployment state for a deployment
    When I get deployment state for "bdd-deploy-state"
    Then the response status should be 200
    And the response JSON at "/item/schemaVersion" should be "v1alpha1"
    And the response JSON at "/item/deployment/deploymentHash" should be "bdd-deploy-state"

Feature: Handoff Token
  As a user I want to create handoff tokens for CLI connections

  Background:
    Given I am authenticated as User A

  Scenario: Mint a handoff token for a deployment
    Given I have a test deployment with hash "bdd-handoff-dep"
    When I mint a handoff token for the stored deployment
    Then the response status should be one of "200, 201"
    And the response JSON at "/item/token" should not be empty

  Scenario: Resolve a valid handoff token
    Given I have a test deployment with hash "bdd-handoff-resolve"
    And I have minted a handoff token for the stored deployment
    When I resolve the stored handoff token
    Then the response status should be 200

  Scenario: Resolve an invalid token returns not found
    When I resolve handoff token "invalid-token-xyz"
    Then the response status should be one of "404, 400"

  Scenario: Handoff token is one-time use
    Given I have a test deployment with hash "bdd-handoff-once"
    And I have minted a handoff token for the stored deployment
    And I resolve the stored handoff token
    When I resolve the stored handoff token
    Then the response status should be one of "404, 400, 410"

Feature: Client Management
  As a user managing API clients
  I want to create, enable, disable, and regenerate client secrets
  So that I can control API access to my resources

  Scenario: Create a new client
    Given I switch to admin user
    When I create a client
    Then the response status should be one of "200, 201"
    And the response JSON should have key "item"

  Scenario: Disable an active client
    Given I switch to admin user
    And I have created a client
    When I disable the stored client
    Then the response status should be 200

  Scenario: Enable a disabled client
    Given I switch to admin user
    And I have created a client
    And I disable the stored client
    When I enable the stored client
    Then the response status should be 200

  Scenario: Regenerate client secret
    Given I switch to admin user
    And I have created a client
    When I update the stored client
    Then the response status should be 200

  Scenario: Cannot disable an already disabled client
    Given I switch to admin user
    And I have created a client
    And I disable the stored client
    When I disable the stored client
    Then the response status should be 400

  Scenario: Cannot enable an already active client
    Given I switch to admin user
    And I have created a client
    When I enable the stored client
    Then the response status should be 400

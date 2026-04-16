Feature: Marketplace Public
  As a visitor I want to browse marketplace templates and categories

  Scenario: List marketplace categories
    Given I switch to anonymous user
    When I list marketplace categories
    Then the response status should be 200

  Scenario: List approved templates
    Given I switch to anonymous user
    When I list marketplace templates
    Then the response status should be 200

  Scenario: Get template detail by slug
    Given I am authenticated as User A
    And I have created a marketplace template with slug "bdd-public-detail"
    And I switch to admin user
    And I submit template "bdd-public-detail" for review
    And I approve the stored template
    And I switch to anonymous user
    When I get marketplace template by slug "bdd-public-detail"
    Then the response status should be 200

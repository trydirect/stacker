Feature: Marketplace Creator
  As a template creator I want to manage my marketplace templates

  Background:
    Given I am authenticated as User A

  Scenario: Create a draft template
    When I create a marketplace template with slug "bdd-creator-new"
    Then the response status should be one of "200, 201"
    And the response JSON should have key "item"

  Scenario: Update template metadata
    Given I have created a marketplace template with slug "bdd-creator-update"
    When I update the stored template with name "Updated Name"
    Then the response status should be 200

  Scenario: Submit template for review
    Given I have created a marketplace template with slug "bdd-creator-submit"
    When I submit the stored template for review
    Then the response status should be 200

  Scenario: List my templates
    Given I have created a marketplace template with slug "bdd-creator-mine"
    When I list my marketplace templates
    Then the response status should be 200

  Scenario: Resubmit after needs-changes
    Given I have created a marketplace template with slug "bdd-creator-resub"
    And I submit the stored template for review
    And I switch to admin user
    And I request changes for the stored template with reason "Fix dockerfile"
    And I switch to User A
    When I resubmit the stored template with version "1.1.0"
    Then the response status should be 200

  Scenario: Get my vendor profile
    When I get my vendor profile
    Then the response status should be 200

  Scenario: Create vendor onboarding link
    When I create vendor onboarding link
    Then the response status should be 200

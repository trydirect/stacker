Feature: Marketplace Admin
  As an admin I want to review and manage marketplace templates

  Background:
    Given I am authenticated as User A

  Scenario: List submitted templates
    Given I have created a marketplace template with slug "bdd-admin-list"
    And I submit the stored template for review
    And I switch to admin user
    When I list submitted templates
    Then the response status should be 200

  Scenario: Get template detail as admin
    Given I have created a marketplace template with slug "bdd-admin-detail"
    And I submit the stored template for review
    And I switch to admin user
    When I get admin template detail for the stored template
    Then the response status should be 200

  Scenario: Approve a template
    Given I have created a marketplace template with slug "bdd-admin-approve"
    And I submit the stored template for review
    And I switch to admin user
    When I approve the stored template
    Then the response status should be 200

  Scenario: Reject a template
    Given I have created a marketplace template with slug "bdd-admin-reject"
    And I submit the stored template for review
    And I switch to admin user
    When I reject the stored template with reason "Does not meet standards"
    Then the response status should be 200

  Scenario: Request changes for a template
    Given I have created a marketplace template with slug "bdd-admin-changes"
    And I submit the stored template for review
    And I switch to admin user
    When I request changes for the stored template with reason "Fix documentation"
    Then the response status should be 200

  Scenario: Run security scan on a template
    Given I have created a marketplace template with slug "bdd-admin-scan"
    And I submit the stored template for review
    And I switch to admin user
    When I run security scan for the stored template
    Then the response status should be 200

  Scenario: Update template pricing
    Given I have created a marketplace template with slug "bdd-admin-price"
    And I submit the stored template for review
    And I switch to admin user
    When I update pricing for the stored template with price 9.99 and billing "one_time"
    Then the response status should be 200

  Scenario: Update template verifications
    Given I have created a marketplace template with slug "bdd-admin-verify"
    And I submit the stored template for review
    And I switch to admin user
    When I update verifications for the stored template with security_reviewed true
    Then the response status should be 200

  Scenario: Unapprove a template
    Given I have created a marketplace template with slug "bdd-admin-unapp"
    And I submit the stored template for review
    And I switch to admin user
    And I approve the stored template
    When I unapprove the stored template
    Then the response status should be 200

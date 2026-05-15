Feature: Agreement Management
  As a user I want to sign agreements
  As an admin I want to manage compliance documents

  Background:
    Given I am authenticated as User A

  Scenario: Admin creates an agreement
    Given I switch to admin user
    When I create an agreement with name "Terms of Service" and text "You agree to our terms..."
    Then the response status should be one of "200, 201"
    And the response JSON should have key "item"

  Scenario: Admin gets an agreement
    Given I switch to admin user
    And I have created an agreement with name "Privacy Policy"
    When I get the stored agreement as admin
    Then the response status should be 200

  Scenario: User signs an agreement
    Given I switch to admin user
    And I have created an agreement with name "User Agreement"
    And I switch to User A
    When I sign the stored agreement
    Then the response status should be one of "200, 201"

  Scenario: User checks if agreement is signed
    Given I switch to admin user
    And I have created an agreement with name "Data Policy"
    And I switch to User A
    And I sign the stored agreement
    When I check if the stored agreement is accepted
    Then the response status should be 200

  Scenario: Cannot sign same agreement twice
    Given I switch to admin user
    And I have created an agreement with name "Duplicate Test"
    And I switch to User A
    And I sign the stored agreement
    When I sign the stored agreement
    Then the response status should be one of "400, 409, 422"

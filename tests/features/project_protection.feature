Feature: Project Protection
  Protected projects cannot be deleted. Protection can be toggled on/off.
  Disabling protection requires re-entering the project name for confirmation.

  Scenario: New project is unprotected by default
    Given I am authenticated as User A
    When I create a project with stack code "unprotected-app"
    Then the response status should be 200
    And the response JSON at "/item/is_protected" should be false

  Scenario: Enable protection on a project
    Given I am authenticated as User A
    And I have created a project with stack code "protect-me"
    When I set protection to true on the stored project
    Then the response status should be 200
    And the response JSON at "/item/is_protected" should be true

  Scenario: Cannot delete a protected project
    Given I am authenticated as User A
    And I have created a project with stack code "locked-project"
    And I set protection to true on the stored project
    When I delete the stored project
    Then the response status should be 403

  Scenario: Disable protection requires correct project name
    Given I am authenticated as User A
    And I have created a project with stack code "confirm-unlock"
    And I set protection to true on the stored project
    When I disable protection on the stored project with confirmation name "wrong-name"
    Then the response status should be 400

  Scenario: Disable protection with correct name allows deletion
    Given I am authenticated as User A
    And I have created a project with stack code "unlock-and-delete"
    And I set protection to true on the stored project
    When I disable protection on the stored project with confirmation name "unlock-and-delete"
    Then the response status should be 200
    And the response JSON at "/item/is_protected" should be false
    When I delete the stored project
    Then the response status should be 200

  Scenario: Protected project shows active resource counts in delete rejection
    Given I am authenticated as User A
    And I have created a project with stack code "resource-check"
    And the stored project has 2 active deployments and 1 server
    When I set protection to true on the stored project
    And I try to delete the stored project
    Then the response status should be 403
    And the response JSON at "/reasons/active_deployments" should be 2
    And the response JSON at "/reasons/active_servers" should be 1

  Scenario: User B cannot toggle protection on User A's project
    Given I am authenticated as User A
    And I have created a project with stack code "no-touch"
    When I switch to User B
    And I set protection to true on the stored project
    Then the response status should be 404

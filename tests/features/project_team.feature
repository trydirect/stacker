Feature: Project Team & Sharing
  Project owners can share projects with other users as viewers.
  Shared projects appear in the member's shared project list.

  Scenario: Share project with a member as viewer
    Given I am authenticated as User A
    And I have created a project with stack code "team-project"
    When I add member "other_user_id" with role "viewer" to the stored project
    Then the response status should be 200

  Scenario: List project members
    Given I am authenticated as User A
    And I have created a project with stack code "team-list"
    And I add member "other_user_id" with role "viewer" to the stored project
    When I list members of the stored project
    Then the response status should be 200
    And the response JSON list should have at least 1 items

  Scenario: Remove a project member
    Given I am authenticated as User A
    And I have created a project with stack code "team-remove"
    And I add member "other_user_id" with role "viewer" to the stored project
    When I remove member "other_user_id" from the stored project
    Then the response status should be 204

  Scenario: Reject unsupported member role
    Given I am authenticated as User A
    And I have created a project with stack code "team-role-test"
    When I add member "other_user_id" with role "admin" to the stored project
    Then the response status should be 400

  Scenario: Shared project appears in member's shared list
    Given I am authenticated as User A
    And I have created a project with stack code "shared-visible"
    And I add member "other_user_id" with role "viewer" to the stored project
    When I switch to User B
    And I send a GET request to "/project/shared"
    Then the response status should be 200
    And the response JSON should have key "list"

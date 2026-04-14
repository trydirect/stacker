Feature: Project CRUD Operations
  Users can create, read, update, list, and delete projects.
  Projects are owned by the authenticated user and isolated between users.

  Scenario: Create a project with valid data
    Given I am authenticated as User A
    When I create a project with stack code "my-test-app"
    Then the response status should be 200
    And the response JSON should have key "item"
    And the response JSON at "/item/name" should be "my-test-app"
    And I store the response JSON "/item/id" as "project_id"

  Scenario: Create a project with minimal data
    Given I am authenticated as User A
    When I create a project with stack code "minimal-app"
    Then the response status should be 200
    And the response JSON at "/item/name" should be "minimal-app"

  Scenario: Accept project with short stack code (validation not enforced)
    Given I am authenticated as User A
    When I create a project with stack code "ab"
    Then the response status should be 200

  Scenario: Accept project with reserved stack code (validation not enforced)
    Given I am authenticated as User A
    When I create a project with stack code "root"
    Then the response status should be 200

  Scenario: Get own project by ID
    Given I am authenticated as User A
    And I have created a project with stack code "get-test-project"
    When I send a GET request to the stored "project_id" at "/project/{id}"
    Then the response status should be 200
    And the response JSON at "/item/name" should be "get-test-project"

  Scenario: List own projects
    Given I am authenticated as User A
    And I have created a project with stack code "list-project-one"
    And I have created a project with stack code "list-project-two"
    When I send a GET request to "/project"
    Then the response status should be 200
    And the response JSON should have key "list"
    And the response JSON list should have at least 2 items

  Scenario: Update project metadata
    Given I am authenticated as User A
    And I have created a project with stack code "update-me"
    When I update the stored project with stack code "updated-code"
    Then the response status should be 200
    And the response JSON at "/item/name" should be "updated-code"

  Scenario: Delete own project
    Given I am authenticated as User A
    And I have created a project with stack code "delete-me"
    When I delete the stored project
    Then the response status should be 200
    When I send a GET request to the stored "project_id" at "/project/{id}"
    Then the response status should be 404

  Scenario: User B cannot access User A's project
    Given I am authenticated as User A
    And I have created a project with stack code "private-project"
    When I switch to User B
    And I send a GET request to the stored "project_id" at "/project/{id}"
    Then the response status should be 404

  Scenario: User B cannot update User A's project
    Given I am authenticated as User A
    And I have created a project with stack code "protected-project"
    When I switch to User B
    And I update the stored project with stack code "hijacked"
    Then the response status should be 400

  Scenario: User B cannot delete User A's project
    Given I am authenticated as User A
    And I have created a project with stack code "no-delete"
    When I switch to User B
    And I delete the stored project
    Then the response status should be one of "400, 403"

  Scenario: Each user sees only their own projects
    Given I am authenticated as User A
    And I have created a project with stack code "user-a-project"
    When I switch to User B
    And I send a GET request to "/project"
    Then the response status should be 200
    And the response JSON list should not contain project "user-a-project"

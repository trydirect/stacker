Feature: Backup management (WS-1)
  Users can manage backup destinations and jobs through the REST API.
  Destinations and jobs are owner-gated; secrets (vault_path) never leak.

  Scenario: Create a destination and a job
    Given I am authenticated as User A
    And I have created a project with stack code "backup-app"
    When I create a backup destination "primary" at "https://api.backup.example"
    Then the response status should be 201
    When I create a backup job "nightly" with source "postgres" targeting "shop-postgres-1"
    Then the response status should be 201
    And the response JSON at "/item/source" should be "postgres"
    And the response JSON at "/item/name" should be "nightly"

  Scenario: List jobs for a project
    Given I am authenticated as User A
    And I have created a project with stack code "backup-app"
    And I have created a backup destination "primary" at "https://api.backup.example"
    And I have created a backup job "nightly" with source "postgres" targeting "shop-postgres-1"
    When I list backup jobs
    Then the response status should be 200
    And the job list contains "nightly"

  Scenario: vault_path is never leaked
    Given I am authenticated as User A
    And I have created a backup destination "primary" at "https://api.backup.example"
    When I list backup destinations
    Then the response status should be 200
    And the response does not contain "vaultPath"
    And the response does not contain "backup/primary"

  Scenario: An invalid source is rejected
    Given I am authenticated as User A
    And I have created a project with stack code "backup-app"
    And I have created a backup destination "primary" at "https://api.backup.example"
    When I create a backup job "bad" with source "oracle" targeting "x"
    Then the response status should be 400

  Scenario: Another user cannot read my backup job
    Given I am authenticated as User A
    And I have created a project with stack code "backup-app"
    And I have created a backup destination "primary" at "https://api.backup.example"
    And I have created a backup job "nightly" with source "postgres" targeting "shop-postgres-1"
    When I switch to User B
    And I fetch the stored backup job
    Then the response status should be one of "403,404"

  Scenario: Another user cannot delete my backup job
    Given I am authenticated as User A
    And I have created a project with stack code "backup-app"
    And I have created a backup destination "primary" at "https://api.backup.example"
    And I have created a backup job "nightly" with source "postgres" targeting "shop-postgres-1"
    When I switch to User B
    And I delete the stored backup job
    Then the response status should be one of "403,404"

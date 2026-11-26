Feature: GitHub repo connect (WS-G)
  Users can connect repositories for auto-deploy. Repos are owner-gated.

  Scenario: Connect a repo and list it
    Given I am authenticated as User A
    And I have created a project with stack code "github-app"
    When I connect a github repo "acme/api" on branch "main"
    Then the response status should be 201
    When I list github repos
    Then the response status should be 200
    And the repo list contains "acme/api"

  Scenario: Another user cannot read my repo
    Given I am authenticated as User A
    And I have created a project with stack code "github-app"
    And I have connected a github repo "acme/api" on branch "main"
    When I switch to User B
    And I fetch the stored github repo
    Then the response status should be one of "403,404"

  Scenario: Another user cannot delete my repo
    Given I am authenticated as User A
    And I have created a project with stack code "github-app"
    And I have connected a github repo "acme/api" on branch "main"
    When I switch to User B
    And I delete the stored github repo
    Then the response status should be one of "403,404"

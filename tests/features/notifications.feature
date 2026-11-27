Feature: Notification channels (WS-N)
  Users can manage notification channels. Channel secrets (vault_path) never
  leak in API responses.

  Scenario: Create a channel and list it
    Given I am authenticated as User A
    And I have created a project with stack code "notify-app"
    When I create a notification channel "ops" of type "webhook"
    Then the response status should be 201
    When I list notification channels
    Then the response status should be 200
    And the channel list contains "ops"

  Scenario: The channel secret is never leaked
    Given I am authenticated as User A
    And I have created a project with stack code "notify-app"
    And I have created a notification channel "ops" of type "webhook"
    When I list notification channels
    Then the response status should be 200
    And the response does not contain "vaultPath"
    And the response does not contain "hooks.example/x"

  Scenario: An invalid channel type is rejected
    Given I am authenticated as User A
    And I have created a project with stack code "notify-app"
    When I create a notification channel "bad" of type "pigeon"
    Then the response status should be one of "400,422"

  Scenario: Another user cannot delete my channel
    Given I am authenticated as User A
    And I have created a project with stack code "notify-app"
    And I have created a notification channel "ops" of type "webhook"
    When I switch to User B
    And I delete the stored notification channel
    Then the response status should be one of "403,404"

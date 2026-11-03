@serial
Feature: Chat Sessions
  As a user I want to manage multiple AI chat sessions (dialogs) on the
  Stack Builder page, each with its own message history stored encrypted at rest.

  Background:
    Given I am authenticated as User A

  Scenario: Create a new chat session
    When I create a chat session titled "Deploy Ghost"
    Then the response status should be one of "200, 201"
    And the response JSON should have key "item"

  Scenario: List my chat sessions
    Given I have created a chat session titled "Session One"
    When I list my chat sessions
    Then the response status should be 200
    And the response JSON should have key "list"

  Scenario: Session list never exposes message content
    Given I have created a chat session titled "Secret Session"
    When I list my chat sessions
    Then the response status should be 200
    And the response body should not contain "messages"

  Scenario: Read messages of a session
    Given I have created a chat session titled "Chatty Session"
    When I get the messages of that session
    Then the response status should be 200
    And the response JSON should have key "item"

  Scenario: Delete a session
    Given I have created a chat session titled "Throwaway Session"
    When I delete that session
    Then the response status should be one of "200, 204"

  Scenario: Reading messages of a non-existent session is a 404
    When I get the messages of session "00000000-0000-0000-0000-000000000000"
    Then the response status should be 404

  Scenario: Append a message to a session
    Given I have created a chat session titled "Ongoing Session"
    When I append a message "How do I deploy?" to that session
    Then the response status should be 200
    And the response JSON should have key "item"

  Scenario: Rename a session
    Given I have created a chat session titled "Old Title"
    When I rename that session to "New Title"
    Then the response status should be 200

  Scenario: Renaming a non-existent session is a 404
    When I rename session "00000000-0000-0000-0000-000000000000" to "Nope"
    Then the response status should be 404

  Scenario: Archiving a session removes it from the default list
    Given I have created a chat session titled "Thread To Archive"
    When I archive that session
    Then the response status should be 200
    When I list my chat sessions
    Then the response body should not contain "Thread To Archive"

  Scenario: Archived sessions appear in the archived list
    Given I have created a chat session titled "Archived Thread Xyz"
    When I archive that session
    And I list my archived chat sessions
    Then the response status should be 200
    And the response body should contain "Archived Thread Xyz"

  Scenario: Unarchiving restores a session to the active list
    Given I have created a chat session titled "Restore Me Abc"
    When I archive that session
    And I unarchive that session
    And I list my chat sessions
    Then the response body should contain "Restore Me Abc"

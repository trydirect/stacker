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

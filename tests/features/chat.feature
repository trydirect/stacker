@serial
Feature: Chat History
  As a user I want to manage chat conversation history

  Background:
    Given I am authenticated as User A

  Scenario: Upsert chat history
    When I upsert chat history with messages
    Then the response status should be one of "200, 201"

  Scenario: Get chat history
    When I upsert and then get chat history
    Then the response status should be 200

  Scenario: Delete chat history
    Given I have upserted chat history
    When I delete chat history
    Then the response status should be one of "200, 204"

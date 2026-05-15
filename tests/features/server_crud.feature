Feature: Server CRUD
  As a user managing servers
  I want to create, list, update, and delete server records
  So that I can manage my infrastructure inventory

  Background:
    Given I am authenticated as User A

  Scenario: List servers
    When I list servers
    Then the response status should be 200

  Scenario: Get servers for a project
    Given I have a test deployment with hash "bdd-srv-proj"
    When I get servers for the stored project
    Then the response status should be 200

  Scenario: Delete preview for a server
    Given I have a test server
    When I get delete preview for the stored server
    Then the response status should be 200

Feature: Cloud CRUD
  As a user managing cloud credentials
  I want to create, list, update, and delete cloud records
  So that I can manage my cloud provider integrations

  Background:
    Given I am authenticated as User A

  Scenario: Create a cloud credential
    When I create a cloud with provider "hetzner" and token "test-token-123"
    Then the response status should be one of "200, 201"
    And the response JSON should have key "item"

  Scenario: List cloud credentials
    Given I have created a cloud with provider "digitalocean"
    When I list clouds
    Then the response status should be 200

  Scenario: Get a specific cloud
    Given I have created a cloud with provider "hetzner"
    When I get the stored cloud
    Then the response status should be 200

  Scenario: Update a cloud credential
    Given I have created a cloud with provider "hetzner"
    When I update the stored cloud with provider "aws"
    Then the response status should be 200

  Scenario: Delete a cloud credential
    Given I have created a cloud with provider "hetzner"
    When I delete the stored cloud
    Then the response status should be one of "200, 204"

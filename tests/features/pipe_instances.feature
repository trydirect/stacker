Feature: Pipe Instances
  Users can create, list, get, update, and delete pipe instances
  that are bound to a specific deployment.

  Background:
    Given I am authenticated as User A
    And I have a test deployment with hash "bdd-deployment-pipes"

  Scenario: Create a pipe instance with target container
    When I create a pipe instance for deployment "bdd-deployment-pipes" with source "wordpress" and target container "mailchimp"
    Then the response status should be 201
    And the response JSON should have key "item"
    And the response JSON at "/item/status" should be "draft"
    And the response JSON at "/item/source_container" should be "wordpress"
    And I store the response JSON "/item/id" as "instance_id"

  Scenario: Create a pipe instance with target URL
    When I create a pipe instance for deployment "bdd-deployment-pipes" with source "app" and target url "https://hooks.example.com/webhook"
    Then the response status should be 201
    And the response JSON at "/item/target_url" should be "https://hooks.example.com/webhook"

  Scenario: Create a pipe instance linked to a template
    Given I have created a pipe template "Instance Template"
    When I create a pipe instance for deployment "bdd-deployment-pipes" with source "app" and target container "svc" linked to the stored template
    Then the response status should be 201

  Scenario: List instances for a deployment
    Given I have created a pipe instance for deployment "bdd-deployment-pipes" with source "src1" and target container "tgt1"
    When I list pipe instances for deployment "bdd-deployment-pipes"
    Then the response status should be 200
    And the response JSON should have key "list"

  Scenario: Get a pipe instance by ID
    Given I have created a pipe instance for deployment "bdd-deployment-pipes" with source "src2" and target container "tgt2"
    When I get the stored pipe instance
    Then the response status should be 200
    And the response JSON at "/item/source_container" should be "src2"

  Scenario: Update pipe instance status to active
    Given I have created a pipe instance for deployment "bdd-deployment-pipes" with source "src3" and target container "tgt3"
    When I update the stored pipe instance status to "active"
    Then the response status should be 200
    And the response JSON at "/item/status" should be "active"

  Scenario: Update pipe instance status to paused
    Given I have created a pipe instance for deployment "bdd-deployment-pipes" with source "src4" and target container "tgt4"
    When I update the stored pipe instance status to "active"
    And I update the stored pipe instance status to "paused"
    Then the response status should be 200
    And the response JSON at "/item/status" should be "paused"

  Scenario: Reject invalid pipe instance status
    Given I have created a pipe instance for deployment "bdd-deployment-pipes" with source "src5" and target container "tgt5"
    When I update the stored pipe instance status to "invalid_status"
    Then the response status should be 400

  Scenario: Delete a pipe instance
    Given I have created a pipe instance for deployment "bdd-deployment-pipes" with source "del-src" and target container "del-tgt"
    When I delete the stored pipe instance
    Then the response status should be 200
    When I get the stored pipe instance
    Then the response status should be 404

  Scenario: Reject instance with empty deployment hash
    When I create a pipe instance with empty deployment hash
    Then the response status should be 400

  Scenario: Reject instance with empty source container
    When I create a pipe instance with empty source container
    Then the response status should be 400

  Scenario: Reject instance with no target
    When I create a pipe instance with no target
    Then the response status should be 400

  Scenario: Reject instance for non-existent deployment
    When I create a pipe instance for deployment "nonexistent-deploy-hash" with source "app" and target container "svc"
    Then the response status should be 404

  Scenario: User B cannot access User A's pipe instances
    Given I have created a pipe instance for deployment "bdd-deployment-pipes" with source "private-src" and target container "private-tgt"
    When I switch to User B
    And I get the stored pipe instance
    Then the response status should be 404

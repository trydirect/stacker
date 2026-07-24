Feature: Pipe Templates
  Users can create, list, get, and delete reusable pipe templates.
  Templates define source-to-target integration patterns.

  Scenario: Create a pipe template with valid data
    Given I am authenticated as User A
    When I create a pipe template "WordPress to Mailchimp"
    Then the response status should be 201
    And the response JSON should have key "item"
    And the response JSON at "/item/name" should be "WordPress to Mailchimp"
    And the response JSON at "/item/source_app_type" should be "wordpress"
    And the response JSON at "/item/target_app_type" should be "mailchimp"
    And I store the response JSON "/item/id" as "template_id"

  Scenario: List own pipe templates
    Given I am authenticated as User A
    And I have created a pipe template "Template Alpha"
    And I have created a pipe template "Template Beta"
    When I list pipe templates
    Then the response status should be 200
    And the response JSON should have key "list"

  Scenario: Get a pipe template by ID
    Given I am authenticated as User A
    And I have created a pipe template "Get Me Template"
    When I get the stored pipe template
    Then the response status should be 200
    And the response JSON at "/item/name" should be "Get Me Template"

  Scenario: Delete own pipe template
    Given I am authenticated as User A
    And I have created a pipe template "Delete Me Template"
    When I delete the stored pipe template
    Then the response status should be 200
    When I get the stored pipe template
    Then the response status should be 404

  Scenario: User B cannot access User A's private template
    Given I am authenticated as User A
    And I have created a pipe template "Private Template"
    When I switch to User B
    And I get the stored pipe template
    Then the response status should be 404

  Scenario: User B can access User A's public template
    Given I am authenticated as User A
    And I have created a public pipe template "Public Template"
    When I switch to User B
    And I get the stored pipe template
    Then the response status should be 200
    And the response JSON at "/item/name" should be "Public Template"

  Scenario: Reject template with empty name
    Given I am authenticated as User A
    When I create a pipe template with empty name
    Then the response status should be 400

  Scenario: Reject template with empty source_app_type
    Given I am authenticated as User A
    When I create a pipe template with empty source_app_type
    Then the response status should be 400

  Scenario: Reject template with empty target_app_type
    Given I am authenticated as User A
    When I create a pipe template with empty target_app_type
    Then the response status should be 400

  Scenario: User B cannot delete User A's template
    Given I am authenticated as User A
    And I have created a pipe template "No Delete For You"
    When I switch to User B
    And I delete the stored pipe template
    Then the response status should be 404

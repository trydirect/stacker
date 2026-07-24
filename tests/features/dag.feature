Feature: DAG Steps and Edges
  As a user I want to build multi-step DAG pipes with branching and conditions

  Background:
    Given I am authenticated as User A
    And I have a DAG pipe template named "bdd-dag-template"

  # --- Steps CRUD ---

  Scenario: Add a source step to a pipe template
    When I add a DAG step to the template with:
      """
      {"step_type": "source", "name": "Extract Data", "config": {"endpoint": "/api/data"}}
      """
    Then the response status should be 201
    And the response JSON at "/item" should not be empty
    And the response JSON at "/item/step_type" should be "source"
    And the response JSON at "/item/name" should be "Extract Data"

  Scenario: Add a transform step to a pipe template
    When I add a DAG step to the template with:
      """
      {"step_type": "transform", "name": "Map Fields", "config": {"mapping": {"a": "b"}}}
      """
    Then the response status should be 201
    And the response JSON at "/item/step_type" should be "transform"

  Scenario: Add a target step to a pipe template
    When I add a DAG step to the template with:
      """
      {"step_type": "target", "name": "Load Data", "config": {"url": "http://target/api"}}
      """
    Then the response status should be 201
    And the response JSON at "/item/step_type" should be "target"

  Scenario: Add a condition step to a pipe template
    When I add a DAG step to the template with:
      """
      {"step_type": "condition", "name": "Check Status", "config": {"expression": "$.status == 'ok'"}}
      """
    Then the response status should be 201
    And the response JSON at "/item/step_type" should be "condition"

  Scenario: Reject invalid step type
    When I add a DAG step to the template with:
      """
      {"step_type": "invalid_type", "name": "Bad Step", "config": {}}
      """
    Then the response status should be one of "400, 422"

  Scenario: List steps for a pipe template
    Given I have added DAG steps "Source,Transform,Target" to the template
    When I list DAG steps for the template
    Then the response status should be 200
    And the response JSON at "/list" should not be empty

  Scenario: Get a single DAG step
    Given I have added a DAG step "Fetch" of type "source" to the template
    When I get the stored DAG step
    Then the response status should be 200
    And the response JSON at "/item/name" should be "Fetch"

  Scenario: Update a DAG step
    Given I have added a DAG step "Old Name" of type "transform" to the template
    When I update the stored DAG step with:
      """
      {"name": "New Name", "config": {"updated": true}}
      """
    Then the response status should be 200
    And the response JSON at "/item/name" should be "New Name"

  Scenario: Delete a DAG step
    Given I have added a DAG step "ToDelete" of type "target" to the template
    When I delete the stored DAG step
    Then the response status should be one of "200, 204"

  # --- Edges CRUD ---

  Scenario: Add an edge between two steps
    Given I have added DAG steps "Source,Target" to the template
    When I add a DAG edge from step "Source" to step "Target"
    Then the response status should be 201
    And the response JSON at "/item" should not be empty

  Scenario: Add a conditional edge
    Given I have added DAG steps "Source,BranchA,BranchB" to the template
    When I add a DAG edge from step "Source" to step "BranchA" with condition:
      """
      {"expression": "$.status == 'ok'"}
      """
    Then the response status should be 201

  Scenario: List edges for a pipe template
    Given I have added DAG steps "S,T" to the template
    And I have added a DAG edge from step "S" to step "T"
    When I list DAG edges for the template
    Then the response status should be 200
    And the response JSON at "/list" should not be empty

  Scenario: Delete an edge
    Given I have added DAG steps "S,T" to the template
    And I have added a DAG edge from step "S" to step "T"
    When I delete the stored DAG edge
    Then the response status should be one of "200, 204"

  Scenario: Reject edge creating a cycle
    Given I have added DAG steps "A,B" to the template
    And I have added a DAG edge from step "A" to step "B"
    When I add a DAG edge from step "B" to step "A"
    Then the response status should be one of "400, 409, 422"

  # --- DAG Validation ---

  Scenario: Validate a well-formed DAG
    Given I have added DAG steps "Source,Transform,Target" to the template
    And I have added a DAG edge from step "Source" to step "Transform"
    And I have added a DAG edge from step "Transform" to step "Target"
    When I validate the DAG for the template
    Then the response status should be 200
    And the response JSON at "/item/valid" should be "true"

  Scenario: Cross-user cannot access another user's DAG steps
    Given I have added a DAG step "Private" of type "source" to the template
    When I switch to User B
    And I list DAG steps for the template
    Then the response status should be one of "403, 404"

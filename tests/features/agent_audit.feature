Feature: Agent Audit Log
  As a service operator I can ingest and query audit events
  via the /api/v1/agent/audit endpoints.

  Scenario: Ingest audit events with valid internal key
    Given I am authenticated as User A
    When I ingest audit events for installation "bdd-audit-hash"
    Then the response status should be 200

  Scenario: Ingest audit events with invalid internal key
    Given I am authenticated as User A
    When I ingest audit events with invalid internal key
    Then the response status should be 401

  # Querying is scoped to deployments the caller owns, so the installation
  # must belong to User A. Without that link the query is a cross-tenant read:
  # `installation_hash` was previously optional, and omitting it returned
  # recent events across every installation to any authenticated user.
  Scenario: Query audit events
    Given I am authenticated as User A
    And I have a test deployment with hash "bdd-audit-query"
    And I have ingested audit events for installation "bdd-audit-query"
    When I query audit events for installation "bdd-audit-query"
    Then the response status should be 200

  Scenario: Query audit events with event_type filter
    Given I am authenticated as User A
    And I have a test deployment with hash "bdd-audit-filter"
    And I have ingested audit events for installation "bdd-audit-filter"
    When I query audit events for installation "bdd-audit-filter" with event type "deploy_start"
    Then the response status should be 200

  Scenario: Ingest empty batch
    Given I am authenticated as User A
    When I ingest empty audit batch for installation "bdd-audit-empty"
    Then the response status should be 200

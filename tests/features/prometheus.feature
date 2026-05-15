Feature: Prometheus Metrics
  As a platform operator
  I want Prometheus-compatible metrics exposed at /metrics
  So that I can monitor pipe executions, DAG runs, and system health

  # ── Metrics Endpoint ────────────────────────────────────────

  Scenario: Metrics endpoint returns Prometheus text format
    When I send a GET request to "/metrics"
    Then the response status should be 200
    And the response content-type should contain "text/plain"
    And the response body should contain "# HELP"
    And the response body should contain "# TYPE"

  Scenario: Metrics include HTTP request counters
    When I send a GET request to "/health_check"
    And I send a GET request to "/metrics"
    Then the response status should be 200
    And the response body should contain "http_requests_total"

  Scenario: Metrics include HTTP request duration histogram
    When I send a GET request to "/metrics"
    Then the response status should be 200
    And the response body should contain "http_request_duration_seconds"

  # ── Pipe Execution Metrics ──────────────────────────────────

  Scenario: Metrics track pipe execution counts
    When I send a GET request to "/metrics"
    Then the response status should be 200
    And the response body should contain "pipe_executions_total"

  Scenario: Metrics track pipe execution duration
    When I send a GET request to "/metrics"
    Then the response status should be 200
    And the response body should contain "pipe_execution_duration_seconds"

  # ── DAG Execution Metrics ───────────────────────────────────

  Scenario: Metrics track DAG execution counts
    When I send a GET request to "/metrics"
    Then the response status should be 200
    And the response body should contain "dag_executions_total"

  Scenario: Metrics track DAG step counts
    When I send a GET request to "/metrics"
    Then the response status should be 200
    And the response body should contain "dag_steps_total"

  # ── System Gauges ───────────────────────────────────────────

  Scenario: Metrics include active pipe instance gauge
    When I send a GET request to "/metrics"
    Then the response status should be 200
    And the response body should contain "active_pipe_instances"

  Scenario: Metrics include active agent gauge
    When I send a GET request to "/metrics"
    Then the response status should be 200
    And the response body should contain "active_agents"

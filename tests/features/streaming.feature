Feature: Streaming Protocols
  As a platform operator
  I want WebSocket and streaming step types in my DAG pipelines
  So that I can build real-time data integration flows

  Background:
    Given I am authenticated as User A

  # ── WebSocket DAG Step Types ───────────────────────────

  Scenario: Create a ws_source DAG step
    Given I have a DAG pipe template named "bdd-stream-ws-src"
    When I add a DAG step to the template with:
      """
      {"name": "WS Ingest", "step_type": "ws_source", "config": {"url": "ws://localhost:9090/feed", "reconnect_interval_ms": 5000}}
      """
    Then the response status should be 201
    And the response body should contain "ws_source"

  Scenario: Create a ws_target DAG step
    Given I have a DAG pipe template named "bdd-stream-ws-tgt"
    When I add a DAG step to the template with:
      """
      {"name": "WS Publish", "step_type": "ws_target", "config": {"url": "ws://localhost:9091/sink", "message_format": "json"}}
      """
    Then the response status should be 201
    And the response body should contain "ws_target"

  Scenario: Create an http_stream_source DAG step (SSE)
    Given I have a DAG pipe template named "bdd-stream-sse"
    When I add a DAG step to the template with:
      """
      {"name": "SSE Feed", "step_type": "http_stream_source", "config": {"url": "http://localhost:8080/events", "event_filter": "data_update"}}
      """
    Then the response status should be 201
    And the response body should contain "http_stream_source"

  Scenario: Reject invalid streaming step type
    Given I have a DAG pipe template named "bdd-stream-invalid"
    When I add a DAG step to the template with:
      """
      {"name": "Bad Step", "step_type": "invalid_stream", "config": {"url": "ws://localhost:9090/feed"}}
      """
    Then the response status should be 400

  # ── DAG with Streaming Steps ───────────────────────────

  Scenario: Build a DAG with ws_source → transform → target
    Given I have a DAG pipe template named "bdd-stream-dag"
    And I have added a DAG step "WsSource" of type "ws_source" to the template
    And I have added a DAG step "Transform" of type "transform" to the template
    And I have added a DAG step "HttpTarget" of type "target" to the template
    And I have added a DAG edge from step "WsSource" to step "Transform"
    And I have added a DAG edge from step "Transform" to step "HttpTarget"
    When I validate the DAG for the template
    Then the response status should be 200
    And the response body should contain "valid"

  Scenario: DAG with ws_source executes and step types are recorded
    Given I have a DAG pipe template named "bdd-stream-exec-dag"
    And I have a DAG pipe instance for that template
    And I have added a DAG step "WsIn" of type "ws_source" with config:
      """
      {"url": "ws://localhost:9090/feed", "output": {"sensor": "temp", "value": 23.5}}
      """
    And I have added a DAG step "MapStep" of type "transform" with config:
      """
      {"mapping": {"sensor": "sensor"}}
      """
    And I have added a DAG step "Sink" of type "ws_target" with config:
      """
      {"url": "ws://localhost:9091/sink", "output": {"delivered": true}}
      """
    And I have added a DAG edge from step "WsIn" to step "MapStep"
    And I have added a DAG edge from step "MapStep" to step "Sink"
    When I execute the DAG with input:
      """
      {}
      """
    Then the response status should be 200
    And the response body should contain "completed"

  # ── Execution Stream (SSE) Endpoint ────────────────────

  Scenario: Execution stream endpoint returns SSE content type
    Given I have a DAG pipe template named "bdd-stream-sse-ep"
    And I have a DAG pipe instance for that template
    When I request the execution stream for the instance
    Then the response status should be 200
    And the response content-type should contain "text/event-stream"

  Scenario: Execution stream emits connection event
    Given I have a DAG pipe template named "bdd-stream-sse-evt"
    And I have a DAG pipe instance for that template
    When I request the execution stream for the instance
    Then the response status should be 200
    And the response body should contain "event: connected"

  # ── gRPC DAG Step Types ─────────────────────────────────

  Scenario: Create a grpc_source DAG step
    Given I have a DAG pipe template named "bdd-stream-grpc-src"
    When I add a DAG step to the template with:
      """
      {"name": "gRPC Ingest", "step_type": "grpc_source", "config": {"endpoint": "http://localhost:50051", "pipe_instance_id": "test", "step_id": "s1"}}
      """
    Then the response status should be 201
    And the response body should contain "grpc_source"

  Scenario: Create a grpc_target DAG step
    Given I have a DAG pipe template named "bdd-stream-grpc-tgt"
    When I add a DAG step to the template with:
      """
      {"name": "gRPC Publish", "step_type": "grpc_target", "config": {"endpoint": "http://localhost:50051", "pipe_instance_id": "test", "step_id": "s2"}}
      """
    Then the response status should be 201
    And the response body should contain "grpc_target"

  Scenario: DAG with grpc_source executes via simulation
    Given I have a DAG pipe template named "bdd-stream-grpc-exec"
    And I have a DAG pipe instance for that template
    And I have added a DAG step "GrpcIn" of type "grpc_source" with config:
      """
      {"endpoint": "http://localhost:50051", "output": {"metric": "cpu", "value": 72.1}}
      """
    And I have added a DAG step "MapGrpc" of type "transform" with config:
      """
      {"mapping": {"metric": "metric"}}
      """
    And I have added a DAG step "GrpcOut" of type "grpc_target" with config:
      """
      {"endpoint": "http://localhost:50051", "output": {"grpc_delivered": true}}
      """
    And I have added a DAG edge from step "GrpcIn" to step "MapGrpc"
    And I have added a DAG edge from step "MapGrpc" to step "GrpcOut"
    When I execute the DAG with input:
      """
      {}
      """
    Then the response status should be 200
    And the response body should contain "completed"

Feature: MCP WebSocket Server
  As an MCP client I want to connect to the Stacker MCP server
  and interact with it using the JSON-RPC 2.0 protocol

  Background:
    Given I am authenticated as User A

  # --- Connection & Handshake ---

  Scenario: WebSocket connection succeeds with authentication
    When I connect to the MCP WebSocket endpoint
    Then the MCP connection should be open

  Scenario: WebSocket connection requires authentication
    When I connect to the MCP WebSocket endpoint without auth
    Then the MCP connection should be rejected

  # --- Initialize ---

  Scenario: Initialize handshake succeeds
    When I connect to the MCP WebSocket endpoint
    And I send an MCP initialize request
    Then the MCP response should have result
    And the MCP result field "protocolVersion" should be "2024-11-05"
    And the MCP result field "serverInfo.name" should be "stacker-mcp"

  Scenario: Initialize with missing params returns error
    When I connect to the MCP WebSocket endpoint
    And I send an MCP request with method "initialize" and no params
    Then the MCP response should have error
    And the MCP error code should be -32602

  # --- Tools List ---

  Scenario: List available tools
    When I connect to the MCP WebSocket endpoint
    And I send an MCP initialize request
    And I send an MCP tools/list request
    Then the MCP response should have result
    And the MCP result should contain a non-empty tools array

  # --- Tools Call ---

  Scenario: Call list_projects tool
    When I connect to the MCP WebSocket endpoint
    And I send an MCP initialize request
    And I send an MCP tools/call request for "list_projects" with arguments:
      """
      {}
      """
    Then the MCP response should have result
    And the MCP tool response should not be an error

  Scenario: Call unknown tool returns error
    When I connect to the MCP WebSocket endpoint
    And I send an MCP initialize request
    And I send an MCP tools/call request for "nonexistent_tool" with arguments:
      """
      {}
      """
    Then the MCP response should have error
    And the MCP error code should be -32001

  Scenario: tools/call with missing params returns error
    When I connect to the MCP WebSocket endpoint
    And I send an MCP initialize request
    And I send an MCP request with method "tools/call" and no params
    Then the MCP response should have error
    And the MCP error code should be -32602

  # --- Unknown Method ---

  Scenario: Unknown JSON-RPC method returns method not found
    When I connect to the MCP WebSocket endpoint
    And I send an MCP request with method "unknown/method" and no params
    Then the MCP response should have error
    And the MCP error code should be -32601

  # --- Invalid JSON ---

  Scenario: Invalid JSON returns parse error
    When I connect to the MCP WebSocket endpoint
    And I send raw MCP text "this is not json"
    Then the MCP response should have error
    And the MCP error code should be -32700

  # --- Notification (no id) ---

  Scenario: Notification without id receives no response
    When I connect to the MCP WebSocket endpoint
    And I send an MCP notification "notifications/initialized"
    Then no MCP response should be received within 500ms

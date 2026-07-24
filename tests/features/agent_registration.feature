Feature: Agent Registration
  As a status panel agent
  I want to register with the Stacker server
  So that I can receive and execute commands

  Scenario: Register a new agent
    When I register an agent with deployment hash "bdd-agent-reg-1"
    Then the response status should be 201
    And the response JSON should have key "data"
    And the response should contain an agent_id
    And the response should contain an agent_token

  Scenario: Re-register an existing agent (idempotent)
    When I register an agent with deployment hash "bdd-agent-reg-2"
    Then the response status should be 201
    When I register an agent with deployment hash "bdd-agent-reg-2"
    Then the response status should be 200

  Scenario: Register agent with capabilities
    When I register an agent with deployment hash "bdd-agent-reg-3" and capabilities "docker,logs,compose"
    Then the response status should be 201
    And the response should contain an agent_id

  Scenario: Register agent with empty deployment hash
    When I register an agent with deployment hash ""
    Then the response status should be 201

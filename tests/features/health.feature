Feature: Health Check
  The health check endpoint verifies that the Stacker server is running
  and returns component health status.

  Scenario: Server responds to health check with status report
    When I send a GET request to "/health_check"
    Then the response status should be one of "200, 503"
    And the response JSON should have key "status"
    And the response JSON should have key "version"
    And the response JSON should have key "components"
    And the response JSON at "/components/database/status" should be "healthy"


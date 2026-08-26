Feature: Docker Compose Auditor
  As a developer evaluating a stack
  I want a graded audit of my docker-compose.yml
  So that I can fix issues before deploying

  Scenario: An insecure compose is graded F with a critical secret finding
    Given the compose fixture "insecure.yml"
    When I audit the compose
    Then the grade is "F"
    And there is a "critical" finding with id "compose.no_secrets"

  Scenario: A clean compose has no critical findings
    Given the compose fixture "clean.yml"
    When I audit the compose
    Then there are no critical findings
    And the score is at least 80

  Scenario: Invalid YAML is reported as one critical finding, not an error
    Given a compose document that is not valid YAML
    When I audit the compose
    Then the grade is "F"
    And there is a "critical" finding with id "compose.invalid_yaml"

Feature: Tier-1 audit checkers
  The exposure, dockerfile, readiness, cost and image checkers each produce a
  graded, actionable report.

  Scenario: Exposure flags a publicly-published database
    Given the compose fixture "public-db.yml"
    When I run the "exposure" checker
    Then the grade is "F"
    And there is a "critical" finding with id "exposure.sensitive_port_public"

  Scenario: Exposure passes a loopback-bound database
    Given the compose fixture "clean.yml"
    When I run the "exposure" checker
    Then there are no critical findings

  Scenario: Dockerfile linter flags an unpinned base running as root
    Given the dockerfile fixture "unpinned.Dockerfile"
    When I run the "dockerfile" checker
    Then there is a "warning" finding with id "dockerfile.unpinned_base"
    And there is a "warning" finding with id "dockerfile.root_user"

  Scenario: Readiness flags operational gaps on a public database
    Given the compose fixture "public-db.yml"
    When I run the "readiness" checker
    Then there is a "warning" finding with id "readiness.no_restart_policy"
    And there is a "critical" finding with id "exposure.sensitive_port_public"

  Scenario: Cost estimator finds the cheapest provider that fits
    Given the compose fixture "clean.yml"
    When I estimate cost with the default pricing
    Then a cheapest provider is returned
    And the quotes are ordered cheapest first

  Scenario: Image inspector fails a missing image
    Given an image reference that does not exist
    When I inspect the image
    Then the grade is "F"
    And there is a "critical" finding with id "image.not_found"

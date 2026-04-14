Feature: Project Apps Management
  Projects can contain multiple apps (microservices). Each app has
  configuration, environment variables, ports, and domain settings.

  Scenario: Create an app in a project
    Given I am authenticated as User A
    And I have created a project with stack code "apps-project"
    When I create an app "nginx" with image "nginx:latest" in the stored project
    Then the response status should be 200
    And the response JSON should have key "item"

  Scenario: List apps in a project
    Given I am authenticated as User A
    And I have created a project with stack code "apps-list-project"
    And I have created an app "redis" with image "redis:7" in the stored project
    And I have created an app "postgres" with image "postgres:16" in the stored project
    When I list apps in the stored project
    Then the response status should be 200
    And the response JSON list should have at least 2 items

  Scenario: Get a specific app by code
    Given I am authenticated as User A
    And I have created a project with stack code "apps-get-project"
    And I have created an app "myapp" with image "myapp:latest" in the stored project
    When I get app "myapp" in the stored project
    Then the response status should be 200

  Scenario: Get app configuration
    Given I am authenticated as User A
    And I have created a project with stack code "apps-config-project"
    And I have created an app "webapp" with image "webapp:1.0" in the stored project
    When I get app config for "webapp" in the stored project
    Then the response status should be 200
    And the response JSON should have key "item"

  Scenario: Update app environment variables
    Given I am authenticated as User A
    And I have created a project with stack code "apps-env-project"
    And I have created an app "api" with image "api:latest" in the stored project
    When I update env vars for app "api" in the stored project with:
      | key       | value     |
      | DB_HOST   | localhost |
      | DB_PORT   | 5432      |
    Then the response status should be 200

  Scenario: Get app environment variables with redaction
    Given I am authenticated as User A
    And I have created a project with stack code "apps-env-read"
    And I have created an app "secure" with image "secure:latest" in the stored project
    And I have set env var "PUBLIC_URL" to "https://example.com" for app "secure"
    And I have set env var "DB_PASSWORD" to "secret123" for app "secure"
    When I get env vars for app "secure" in the stored project
    Then the response status should be 200
    And the response JSON at "/item/variables/PUBLIC_URL" should be "https://example.com"
    And the response JSON at "/item/variables/DB_PASSWORD" should be "[REDACTED]"

  Scenario: Delete an app environment variable
    Given I am authenticated as User A
    And I have created a project with stack code "apps-env-del"
    And I have created an app "svc" with image "svc:latest" in the stored project
    And I have set env var "TEMP_VAR" to "temp" for app "svc"
    When I delete env var "TEMP_VAR" for app "svc" in the stored project
    Then the response status should be 200

  Scenario: Update app ports
    Given I am authenticated as User A
    And I have created a project with stack code "apps-ports"
    And I have created an app "web" with image "web:latest" in the stored project
    When I update ports for app "web" in the stored project with host 8080 container 3000
    Then the response status should be 200

  Scenario: Update app domain
    Given I am authenticated as User A
    And I have created a project with stack code "apps-domain"
    And I have created an app "frontend" with image "frontend:latest" in the stored project
    When I update domain for app "frontend" in the stored project to "app.example.com" with SSL
    Then the response status should be 200

  Scenario: Reject app creation with empty code
    Given I am authenticated as User A
    And I have created a project with stack code "apps-validate"
    When I create an app "" with image "nginx:latest" in the stored project
    Then the response status should be 400

  Scenario: Reject app creation with empty image
    Given I am authenticated as User A
    And I have created a project with stack code "apps-validate2"
    When I create an app "myapp" with image "" in the stored project
    Then the response status should be 400

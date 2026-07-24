Feature: ML Field Matching
  As a platform operator
  I want to match source and target fields intelligently
  So that pipe mappings are generated automatically

  Background:
    Given I am authenticated as User A

  # ── Exact & Case-Insensitive Matching ─────────────────

  Scenario: Exact field name match returns confidence 1.0
    When I request field matching with source fields "name,email" and target fields "name,email"
    Then the response status should be 200
    And the field match for "name" should map to "name"
    And the field match confidence for "name" should be at least 0.99

  Scenario: Case-insensitive match returns high confidence
    When I request field matching with source fields "UserName,Email" and target fields "username,email"
    Then the response status should be 200
    And the field match for "username" should map to "UserName"
    And the field match confidence for "username" should be at least 0.9

  # ── Semantic Alias Matching ───────────────────────────

  Scenario: Semantic alias match for common field variations
    When I request field matching with source fields "fname,lname,phone" and target fields "first_name,last_name,telephone"
    Then the response status should be 200
    And the field match for "first_name" should map to "fname"
    And the field match for "last_name" should map to "lname"

  # ── N-gram Cosine Similarity ──────────────────────────

  Scenario: Similar field names matched via n-gram similarity
    When I request field matching with source fields "customer_email,order_date" and target fields "customerEmail,orderDate"
    Then the response status should be 200
    And the field match for "customerEmail" should map to "customer_email"
    And the field match confidence for "customerEmail" should be at least 0.4

  Scenario: Dissimilar fields are not matched
    When I request field matching with source fields "temperature,humidity" and target fields "user_id,password"
    Then the response status should be 200
    And the field match result should have unmatched target "user_id"
    And the field match result should have unmatched target "password"

  # ── Compound Field Names ──────────────────────────────

  Scenario: Token overlap matches compound field names
    When I request field matching with source fields "shipping_address_line1,billing_zip_code" and target fields "address_line_1,zip_code"
    Then the response status should be 200
    And the field match for "zip_code" should map to "billing_zip_code"

  # ── Edge Cases ────────────────────────────────────────

  Scenario: Empty source fields returns empty mapping
    When I request field matching with source fields "" and target fields "name,email"
    Then the response status should be 200
    And the field match mapping should be empty

  Scenario: Empty target fields returns empty mapping
    When I request field matching with source fields "name,email" and target fields ""
    Then the response status should be 200
    And the field match mapping should be empty

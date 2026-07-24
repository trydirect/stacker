Feature: Rating System
  As a user I want to rate and review products
  As an admin I want to manage ratings

  Background:
    Given I am authenticated as User A

  Scenario: Create a rating
    When I create a rating for object 1 category "Application" with rate 8 and comment "Great app"
    Then the response status should be one of "200, 201"
    And the response JSON should have key "item"

  Scenario: Get own rating
    Given I have created a rating for object 2 category "Application" with rate 7
    When I get the stored rating
    Then the response status should be 200

  Scenario: Update own rating
    Given I have created a rating for object 3 category "Cloud" with rate 5
    When I update the stored rating with rate 9 and comment "Updated review"
    Then the response status should be 200

  Scenario: Soft-delete own rating
    Given I have created a rating for object 4 category "Price" with rate 6
    When I delete the stored rating
    Then the response status should be 200

  Scenario: Cannot rate same object and category twice
    Given I have created a rating for object 5 category "Design" with rate 8
    When I create a rating for object 5 category "Design" with rate 9 and comment "Duplicate"
    Then the response status should be one of "400, 409, 422"

  Scenario: List visible ratings as anonymous
    Given I switch to anonymous user
    When I list ratings
    Then the response status should be 200

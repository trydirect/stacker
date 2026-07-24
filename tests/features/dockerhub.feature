Feature: DockerHub Integration
  As a user I want to search DockerHub namespaces, repositories, and tags

  Background:
    Given I am authenticated as User A

  Scenario: Search DockerHub namespaces
    When I search DockerHub namespaces with query "nginx"
    Then the response status should be 200

  Scenario: List repositories for a namespace
    When I list DockerHub repositories for namespace "library"
    Then the response status should be 200

  Scenario: List tags for a repository
    When I list DockerHub tags for namespace "library" repository "nginx"
    Then the response status should be 200

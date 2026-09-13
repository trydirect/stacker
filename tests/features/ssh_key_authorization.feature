Feature: SSH public key authorization
  As a user who just deployed a cloud server
  I want my public keys appended to authorized_keys through the API
  So that I can SSH into the machine I paid for

  # This endpoint had no test coverage at all, which is how a broken Vault
  # policy and a single-attempt 15s SSH timeout both reached production
  # unnoticed.
  #
  # The scenarios below cover the guard rails, which are deterministic. The
  # retry-while-the-VM-boots behaviour is NOT covered here on purpose: it needs
  # an SSH server that refuses and then accepts, which this harness has no
  # fixture for, and a scenario that cannot actually observe a retry would be
  # worse than none. That policy is unit-tested in
  # src/routes/server/ssh_key.rs (ssh_authorize_retry).

  Background:
    Given I am authenticated as User A
    And I have a test server

  Scenario: A server whose SSH key is not active is rejected before any SSH attempt
    # key_status is 'none' until the deploy stores a key in Vault. Retrying
    # cannot help, so this must fail immediately rather than spending the
    # connection budget.
    When I authorize public key "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBDDTESTKEY bdd@example.com" for the stored server
    Then the response status should be 400
    And the response message should contain "not active"

  Scenario: A malformed public key is rejected
    Given the stored server has an active SSH key
    When I authorize public key "not-a-public-key" for the stored server
    Then the response status should be 400
    And the response message should contain "Invalid public key format"

  Scenario: An empty public key is rejected
    Given the stored server has an active SSH key
    When I authorize public key "" for the stored server
    Then the response status should be 400

  Scenario: A server with an active key but no Vault path is rejected
    # The private key is what Stacker uses to SSH in and append the new key.
    # Without it there is nothing to authorize with.
    Given the stored server has an active SSH key without a Vault path
    When I authorize public key "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBDDTESTKEY bdd@example.com" for the stored server
    Then the response status should be 400
    And the response message should contain "Vault"

  Scenario: A server belonging to another user cannot be targeted
    Given the stored server belongs to another user
    When I authorize public key "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBDDTESTKEY bdd@example.com" for the stored server
    Then the response status should be one of "403,404"

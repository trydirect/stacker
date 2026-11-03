Feature: Bake-time sanitization of a build box
  A marketplace snapshot is taken from the author's own build box, so the
  author's secrets must not survive into the image the buyer clones.
  The author-declared config_contract is the only authority on what is
  sensitive — nothing is guessed from variable names.

  Rule: the .env co-located with the compose file is scrubbed

    Scenario: A contract-declared field is blanked
      Given the contract declares "POSTGRES_PASSWORD" as generated
      And the build box .env contains:
        """
        POSTGRES_PASSWORD=0123456789abcdef0123456789abcdef
        OLLAMA_MODEL=llama3.1
        """
      When the .env is scrubbed for the snapshot
      Then the scrubbed .env has "POSTGRES_PASSWORD" blanked
      And the scrubbed .env still has "OLLAMA_MODEL" set to "llama3.1"

    Scenario: A DSN embedding a declared secret is blanked
      Given the contract declares "POSTGRES_PASSWORD" as generated
      And the build box .env contains:
        """
        POSTGRES_PASSWORD=0123456789abcdef0123456789abcdef
        DATABASE_URL=postgresql://stackpilot:0123456789abcdef0123456789abcdef@db:5432/s
        REDIS_URL=redis://stackpilot-redis:6379
        """
      When the .env is scrubbed for the snapshot
      Then the scrubbed .env has "DATABASE_URL" blanked
      And the scrubbed .env still has "REDIS_URL" set to "redis://stackpilot-redis:6379"
      And the scrubbed .env contains no occurrence of "0123456789abcdef0123456789abcdef"

    Scenario: Without a contract nothing is treated as secret
      Given the contract declares nothing
      And the build box .env contains:
        """
        DB_PASSWORD=0123456789abcdef0123456789abcdef
        ADMIN_USER=admin
        """
      When the .env is scrubbed for the snapshot
      Then the scrubbed .env is unchanged

    Scenario: A short declared value does not blank unrelated lines
      Given the contract declares "PORT_TOKEN" as generated
      And the build box .env contains:
        """
        PORT_TOKEN=8080
        PUBLIC_URL=http://host:8080/app
        """
      When the .env is scrubbed for the snapshot
      Then the scrubbed .env has "PORT_TOKEN" blanked
      And the scrubbed .env still has "PUBLIC_URL" set to "http://host:8080/app"

  Rule: secrets embedded inside compose values become ${VAR} references

    Scenario: The password inside a DSN is parameterized
      Given the contract declares "POSTGRES_PASSWORD" as generated
      And "POSTGRES_PASSWORD" on the build box resolves to "0123456789abcdef0123456789abcdef"
      And the generated compose is:
        """
        services:
          app:
            environment:
              DATABASE_URL: postgresql://stackpilot:0123456789abcdef0123456789abcdef@db:5432/s
        """
      When the compose is sanitized for the snapshot
      Then the sanitized compose contains "DATABASE_URL: postgresql://stackpilot:${POSTGRES_PASSWORD}@db:5432/s"
      And the sanitized compose contains no occurrence of "0123456789abcdef0123456789abcdef"

    Scenario: One secret declared under two protected names aborts the bake
      Given the contract declares "DB_PASSWORD" as generated
      And the contract declares "POSTGRES_PASSWORD" as generated
      And "DB_PASSWORD" on the build box resolves to "0123456789abcdef0123456789abcdef"
      And the generated compose is:
        """
        services:
          db:
            environment:
              POSTGRES_PASSWORD: 0123456789abcdef0123456789abcdef
        """
      When the compose is sanitized for the snapshot
      Then sanitizing fails naming both "DB_PASSWORD" and "POSTGRES_PASSWORD"

    Scenario: A value nobody declared is left alone
      Given the contract declares nothing
      And the generated compose is:
        """
        services:
          app:
            environment:
              PUBLIC_URL: http://host/aaaaaaaaaaaaaaaa
        """
      When the compose is sanitized for the snapshot
      Then the sanitized compose contains "http://host/aaaaaaaaaaaaaaaa"

  Rule: only the project's own volumes are reset

    Scenario: A keep entry carrying shell syntax is refused
      Given the stack keeps the volume matching "oll*ama"
      When the volume reset commands are built and may fail
      Then building the commands is refused

    Scenario: A keep entry matches whole segments, not any substring
      Given the stack keeps the volume matching "ollama"
      When the volume reset commands are built for "/home/trydirect/project"
      Then the commands do not keep every name containing "ollama"

    Scenario: Volume removal never enumerates the whole host
      Given the stack keeps the volume matching "ollama"
      When the volume reset commands are built for "/home/trydirect/project"
      Then the commands list volumes from the project compose
      And the commands scope removal by the compose volume label
      And the commands never list every volume on the host
      And the commands skip the volume matching "ollama"

  Rule: the image keeps a reference only when something will fill it

    Scenario: A variable outside the contract goes back to its value
      Given the contract declares nothing
      And "REGION" on the build box resolves to "fsn1"
      And the generated compose is:
        """
        services:
          app:
            environment:
              REGION: ${REGION}
        """
      When references outside the contract are resolved
      Then the resolved compose contains "REGION: fsn1"
      And no reference is reported as unfillable

    Scenario: A contract field stays a reference
      Given the contract declares "POSTGRES_PASSWORD" as generated
      And "POSTGRES_PASSWORD" on the build box resolves to "aaaaaaaaaaaaaaaa"
      And the generated compose is:
        """
        services:
          db:
            environment:
              POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}
        """
      When references outside the contract are resolved
      Then the resolved compose contains "POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}"
      And the resolved compose contains no occurrence of "aaaaaaaaaaaaaaaa"

    Scenario: Defaults and escaped text need no source
      Given the contract declares nothing
      And the generated compose is:
        """
        services:
          app:
            environment:
              LOG: ${LOG_LEVEL:-info}
              CMD: echo $${HOME}
        """
      When references outside the contract are resolved
      Then the resolved compose contains "${LOG_LEVEL:-info}"
      And the resolved compose contains "$${HOME}"
      And no reference is reported as unfillable

    Scenario: A reference with no value and no default is reported
      Given the contract declares nothing
      And the generated compose is:
        """
        services:
          app:
            environment:
              TOKEN: ${MISSING}
        """
      When references outside the contract are resolved
      Then "MISSING" is reported as unfillable

    Scenario: The required list is drawn from the contract, not the file text
      Given the contract declares "SECRET_KEY" as generated
      And the generated compose is:
        """
        services:
          app:
            environment:
              SECRET_KEY: ${SECRET_KEY}
              LOG: ${LOG_LEVEL:-info}
              CMD: echo $${HOME}
              REGION: ${REGION}
        """
      When the required environment keys are collected
      Then the required keys are exactly "SECRET_KEY"

  Rule: the image carries no access belonging to the author

    Scenario: The author's SSH access is removed
      When the identity reset commands are built
      Then the commands remove "/root/.ssh/authorized_keys"
      And the commands remove "/home/*/.ssh/authorized_keys"

    Scenario: The author's private keys and known hosts are removed
      When the identity reset commands are built
      Then the commands remove "/root/.ssh/id_"
      And the commands remove "known_hosts"

    Scenario: Registry credentials are removed
      When the identity reset commands are built
      Then the commands remove "/root/.docker/config.json"

    Scenario: Machine identity is still stripped
      When the identity reset commands are built
      Then the commands remove "/etc/ssh/ssh_host_*"
      And the commands remove "/etc/machine-id"
      And the commands remove "/var/lib/cloud/instance"

  Rule: a bake that cannot sanitize refuses to publish

    Scenario: An unresolved contract stops the bake
      Given the contract declares nothing
      When the bake checks whether it can sanitize
      Then the bake is refused
      And the refusal mentions "approved"

    Scenario: An unresolved contract may be overridden deliberately
      Given the contract declares nothing
      And unsanitized snapshots are explicitly allowed
      When the bake checks whether it can sanitize
      Then the bake is allowed

    Scenario: A resolved contract lets the bake proceed
      Given the contract declares "SECRET_KEY" as generated
      When the bake checks whether it can sanitize
      Then the bake is allowed

    Scenario: A missing env file is reported but does not stop the bake
      Given the contract declares "SECRET_KEY" as generated
      And the build box has no .env beside the compose
      When the bake checks the values it has to work from
      Then the bake is allowed
      And a warning mentions "--project-dir"

    Scenario: A project without an env file draws no warning
      Given the contract declares nothing
      And the build box has no .env beside the compose
      When the bake checks the values it has to work from
      Then the bake is allowed
      And no warning is raised

  Rule: a large file is written without exceeding the command-length limit

    Scenario: A small file is written in one command
      When a file of 500 bytes is written to the build box
      Then it takes 1 command
      And the first command truncates the file

    Scenario: A large compose is written in appended chunks
      When a file of 300000 bytes is written to the build box
      Then it takes more than one command
      And the first command truncates the file
      And every later command appends
      And every command fits in a single argument

    Scenario: The chunks reassemble into the original file
      When a file of 200000 bytes is written to the build box
      Then decoding the chunks in order yields the original content

  Rule: a failed finalize says what it already did

    Scenario: A failure before anything changed leaves the box usable
      Given the finalize completed "read"
      When the recovery advice is produced
      Then the advice says the bake can be retried
      And the advice does not ask for a fresh build box

    Scenario: A failure after the tear-down is destructive
      Given the finalize completed "read, teardown"
      When the recovery advice is produced
      Then the advice mentions "data volumes"
      And the advice asks for a fresh build box

    Scenario: A failure after the files were rewritten cannot be retried in place
      Given the finalize completed "read, teardown, rewrite compose"
      When the recovery advice is produced
      Then the advice mentions "already sanitized"
      And the advice asks for a fresh build box

    Scenario: A failure after the identity reset locks the operator out
      Given the finalize completed "read, teardown, strip identity"
      When the recovery advice is produced
      Then the advice mentions "no longer accepts"
      And the advice asks for a fresh build box

  Rule: values a buyer would silently lose stop the bake

    Scenario: A service reading through env_file loses its non-contract values
      Given the contract declares "SECRET_KEY" as generated
      And "SECRET_KEY" on the build box resolves to "aaaaaaaaaaaaaaaa"
      And "OLLAMA_MODEL" on the build box resolves to "llama3.1"
      And the generated compose is:
        """
        services:
          app:
            env_file:
              - .env
        """
      When the bake checks what a clone would lose
      Then "OLLAMA_MODEL" is reported as lost
      And "SECRET_KEY" is not reported as lost

    Scenario: A compose without env_file loses nothing
      Given the contract declares nothing
      And "OLLAMA_MODEL" on the build box resolves to "llama3.1"
      And the generated compose is:
        """
        services:
          app:
            environment:
              OLLAMA_MODEL: llama3.1
        """
      When the bake checks what a clone would lose
      Then nothing is reported as lost

  Rule: both spellings of an environment block are covered

    Scenario: The list form is parameterized too
      Given "POSTGRES_PASSWORD" is a protected compose key
      And the generated compose is:
        """
        services:
          db:
            environment:
              - POSTGRES_PASSWORD=aaaaaaaaaaaaaaaa
              - POSTGRES_USER=stackpilot
        """
      When whole-value keys are parameterized
      Then the parameterized compose contains "- POSTGRES_PASSWORD=${POSTGRES_PASSWORD}"
      And the parameterized compose contains "- POSTGRES_USER=stackpilot"

  Rule: the author declares which volumes survive, and the platform checks it

    Scenario: A volume declared fixed survives the bake
      Given the contract declares volume "app_ollama" on service "ollama" as fixed
      When the kept volumes are collected
      Then "app_ollama" is kept

    Scenario: An undeclared volume is reset
      Given the contract declares nothing
      When the kept volumes are collected
      Then nothing is kept

    # Measured on real containers: Postgres stores the password as a SCRAM hash,
    # n8n keeps its encryption key inside database.sqlite, and a Qdrant volume
    # holds only collections because the API key is read from the environment at
    # every start. The secret is absent from all three, so no automated check can
    # separate the volume that must be reset from the one that must be kept. The
    # author knows; the platform does not.
    Scenario: Keeping a volume of a service that regenerates a secret is the author's call
      Given the contract declares volume "kb_qdrant_data" on service "qdrant" as fixed
      And service "qdrant" regenerates "QDRANT__SERVICE__API_KEY"
      When the declaration is checked
      Then the declaration is accepted

    Scenario: A volume of a service without per-buyer secrets is allowed
      Given the contract declares volume "app_ollama" on service "ollama" as fixed
      When the declaration is checked
      Then the declaration is accepted

    Scenario: A name carrying shell syntax is refused
      Given the contract declares volume "oll*ama" on service "ollama" as fixed
      When the declaration is checked
      Then the declaration is refused
      And the refusal names "oll*ama"

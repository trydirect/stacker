# stacker-cli

This crate reserves the name for the [TryDirect Stacker
CLI](https://github.com/trydirect/stacker). It contains no code: the CLI is
not yet published to crates.io, because it currently builds from the same
package as the Stacker server and `cargo install` would make you compile the
whole backend for one binary.

`cargo install stacker-cli` will tell you there is nothing to install. That
is deliberate - it is better than installing something that does nothing.

## Installing the CLI today

```bash
brew install trydirect/tap/stacker
```

or

```bash
curl -fsSL https://get.stacker.my/install.sh | bash
```

Prebuilt binaries for Linux and macOS, x86_64 and arm64, are attached to each
[release](https://github.com/trydirect/stacker/releases).

## What Stacker does

Deploys pre-built self-hosted stacks onto a server you already have, over
SSH, or into your own cloud account with your own API key. The catalogue of
ready-to-run projects is at
[awesome-selfhosted-stacker](https://github.com/trydirect/awesome-selfhosted-stacker).

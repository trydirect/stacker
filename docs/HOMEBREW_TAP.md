# Homebrew tap for the Stacker CLI

Goal: `brew install trydirect/tap/stacker` works on macOS and Linux.

Why a tap and not homebrew-core: homebrew-core applies a notability bar
(roughly tens of forks and watchers, or ~75 stars). This repository has 7
stars, 2 forks and 2 watchers as of 2026-11-12, so a submission there would be
declined. A tap has no such requirement and is entirely under our control.

Nothing has to be built: `.github/workflows/release.yml` already publishes
tarballs on every published release, and each one contains a single binary
named `stacker`.

## 1. Create the repository

The name must be exactly `homebrew-tap` - that is what lets Homebrew resolve
`trydirect/tap`.

    gh repo create trydirect/homebrew-tap --public \
      --description "Homebrew tap for the TryDirect Stacker CLI"
    git clone git@github.com:trydirect/homebrew-tap.git
    cd homebrew-tap
    mkdir -p Formula

As of 2026-11-12 the repository does not exist: the GitHub API returns 404 for
`trydirect/homebrew-tap`.

## 2. Formula/stacker.rb

Values below are real, taken from release v0.3.5 (published 2026-10-19) and
the sha256 of each asset as downloaded on 2026-11-12.

```ruby
class Stacker < Formula
  desc "Deploy pre-built self-hosted stacks to your own server or to a cloud"
  homepage "https://try.direct"
  version "0.3.5"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/trydirect/stacker/releases/download/v0.3.5/stacker-v0.3.5-aarch64-darwin.tar.gz"
      sha256 "defdde63d98656c03e2b4d212604a87ca2cd292b35e5a8c9d34f97190834035d"
    end
    on_intel do
      url "https://github.com/trydirect/stacker/releases/download/v0.3.5/stacker-v0.3.5-x86_64-darwin.tar.gz"
      sha256 "cca9e82396b43302e98bdfe6ac25afbaabd116eccec03c0bb013527d37a6bbdb"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/trydirect/stacker/releases/download/v0.3.5/stacker-v0.3.5-x86_64-linux.tar.gz"
      sha256 "141dd5b4be2286fa061b7e1cc84003fa775ab621637be33f984a401ce79d20b3"
    end
    # Add once the first release built from the aarch64-unknown-linux-gnu
    # matrix entry has published its asset. Do not guess the sha256; take it
    # from the published tarball.
    # on_arm do
    #   url ".../stacker-vX.Y.Z-aarch64-linux.tar.gz"
    #   sha256 "..."
    # end
  end

  def install
    bin.install "stacker"
  end

  test do
    assert_match(/\d+\.\d+\.\d+/, shell_output("#{bin}/stacker --version"))
  end
end
```

The test deliberately matches any version number rather than a specific one -
see the version caveat below.

## 3. Verify before announcing it

    brew audit --strict --online Formula/stacker.rb
    brew install --formula ./Formula/stacker.rb
    stacker --version
    brew uninstall stacker

Then the real path, from a clean machine:

    brew tap trydirect/tap
    brew install trydirect/tap/stacker

## 4. Keeping it current

Automated since 2026-11-13: `release.yml` has a `bump-homebrew-formula` job
that runs after the binaries are uploaded, recomputes the sha256 of all four
assets, rewrites `Formula/stacker.rb` and pushes it to the tap.

It needs `HOMEBREW_TAP_TOKEN` in this repository's secrets - a fine-grained
PAT owned by the `trydirect` organisation, scoped to `trydirect/homebrew-tap`
alone, with `Contents: Read and write`. The default `GITHUB_TOKEN` cannot
reach another repository.

Three properties worth knowing, because each one is a failure mode that was
designed out rather than discovered later:

- **No secret, no failure.** The steps are guarded with
  `if: ${{ env.HOMEBREW_TAP_TOKEN != '' }}`, the same idiom
  `status/.github/workflows/ci.yml` uses for `CONFIG_FIXTURES_TOKEN`. Without
  the secret the job skips and the release still succeeds.
- **A missing asset aborts before anything is written.** The sha256 of every
  platform is fetched first; a release that did not publish all four cannot
  leave the formula half-rewritten. This is not hypothetical - v0.3.5 had no
  `aarch64-linux` asset, and running the script against it fails loudly with
  the file untouched.
- **Re-runnable.** If the formula already names that version the job prints
  "nothing to commit" and exits 0.

Prereleases are skipped: the formula tracks stable.

The PAT expires - fine-grained tokens cap at 366 days. When it does, the job
starts skipping silently, because a missing secret is indistinguishable from
an expired one at the `env != ''` check. If the formula ever lags a release,
check the token first.

## Resolved by v0.3.6 (2026-11-13)

All three blockers this runbook listed are closed, verified against the
published assets rather than assumed:

- **The binary reports its own version again.** v0.3.5 printed
  `0.3.4 (a7aac3f)` because `Cargo.toml` was bumped after the tag was cut.
  v0.3.6 prints `0.3.6 (a2cfd7c)`.
- **Linux arm64 ships.** `stacker-v0.3.6-aarch64-linux.tar.gz` exists and the
  formula's `on_linux`/`on_arm` block is live, not commented out.
- **The glibc floor is 2.34**, read off both Linux binaries. v0.3.5 required
  `GLIBC_2.39` and would not start on Ubuntu 22.04 (2.35) or Debian 12
  (2.36); the release runners are pinned to `ubuntu-22.04` and
  `ubuntu-22.04-arm` now. Never put `ubuntu-latest` back on a job that
  produces a release binary: it inherits that image's glibc and silently
  narrows who can run what ships.

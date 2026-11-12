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

## 4. Keep it current on every release

`release.yml` fires on `release: published` and uploads the three assets with
`gh release upload`. Add one job that runs after `build-and-upload`, computes
the sha256 of each asset and commits the bumped formula to the tap. It needs a
token with write access to the tap repository - the default `GITHUB_TOKEN` is
scoped to this repository only, so add a PAT as a secret, for example
`HOMEBREW_TAP_TOKEN`.

Sketch, not tested:

```yaml
  bump-tap:
    needs: build-and-upload
    runs-on: ubuntu-22.04
    steps:
      - name: Checkout the tap
        uses: actions/checkout@v4
        with:
          repository: trydirect/homebrew-tap
          token: ${{ secrets.HOMEBREW_TAP_TOKEN }}
      - name: Rewrite the formula
        run: |
          VERSION="${GITHUB_REF_NAME#v}"
          for a in aarch64-darwin x86_64-darwin x86_64-linux; do
            url="https://github.com/trydirect/stacker/releases/download/v${VERSION}/stacker-v${VERSION}-${a}.tar.gz"
            sha=$(curl -sL "$url" | shasum -a 256 | cut -d' ' -f1)
            echo "$a $sha"
          done
          # substitute version, urls and sha256 values into Formula/stacker.rb
      - name: Commit
        run: |
          git config user.name "stacker-release"
          git config user.email "noreply@try.direct"
          git commit -am "stacker ${GITHUB_REF_NAME}" && git push
```

Pin `ubuntu-22.04`, not `ubuntu-latest`: latest moved to Ubuntu 26.04 and has
already broken three of our workflows this month.

## Two things to fix before this is announced

**The published binary reports the wrong version.** The v0.3.5 darwin-arm64
asset prints `0.3.4 (a7aac3f)`. The release workflow builds from the tag, so
`Cargo.toml` still said 0.3.4 when v0.3.5 was tagged - the bump landed after.
A formula that declares 0.3.5 while `stacker --version` says 0.3.4 will
confuse anyone who checks, and `brew audit` conventions assume the two agree.
Bump the version in `Cargo.toml` before tagging, or derive it from the tag.

**There is no Linux arm64 build.** The matrix in `release.yml:19-32` covers
`x86_64-unknown-linux-gnu`, `x86_64-apple-darwin` and `aarch64-apple-darwin`.
Our audience self-hosts, and arm64 VPS instances are now the cheap default at
several providers, so `brew install` and the curl installer both fail there.
Adding `aarch64-unknown-linux-gnu` to the matrix is the single widest
compatibility win available.

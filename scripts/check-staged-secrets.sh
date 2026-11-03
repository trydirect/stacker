#!/usr/bin/env bash
#
# Refuse a commit that introduces a concrete secret value.
#
# This repository is public: a value committed here is disclosed permanently,
# and rotating it afterwards does not un-publish it. A real password reached
# `src/cli/generator/compose.rs` this way — the scanner configured in
# .pre-commit-config.yaml was never installed, so nothing checked.
#
# Deliberately narrow, because a noisy hook gets bypassed:
#   1. an assignment to a secret-named key whose value looks concrete
#   2. a long hex/base64 blob used as a value
#
# Placeholders, ${REFERENCES} and empty values are fine — those are what test
# fixtures and templates should contain.
#
# Opt out for a line that is genuinely not a secret:
#     API_KEY=abcdef0123456789abcdef  # pragma: allowlist secret
#
# Scan the whole tree instead of the staged diff with --all.

set -uo pipefail

RED=$'\033[31m'; YELLOW=$'\033[33m'; RESET=$'\033[0m'
[ -t 1 ] || { RED=''; YELLOW=''; RESET=''; }

if [ "${1:-}" = "--all" ]; then
    added=$(git grep -n '' -- . | sed 's/^/+/')
else
    # Only added lines of the staged diff, with their file and line numbers.
    added=$(git diff --cached --unified=0 --no-color -- . | awk '
        /^\+\+\+ b\// { file = substr($0, 7); next }
        /^@@/ {
            match($0, /\+[0-9]+/)
            line = substr($0, RSTART + 1, RLENGTH - 1) - 1
            next
        }
        /^\+/ && !/^\+\+\+/ { line++; print file ":" line ":" substr($0, 2) }
    ')
fi

[ -n "$added" ] || exit 0

SECRET_KEY_RE='(PASSWORD|PASSWD|PWD|SECRET|TOKEN|API_?KEY|ACCESS_KEY|CREDENTIAL|PRIVATE_KEY|MASTERKEY|SIGNING_KEY|ENCRYPTION_KEY)'

findings=$(printf '%s\n' "$added" | awk -v keyre="$SECRET_KEY_RE" '
    # An explicit opt-out wins.
    /pragma: allowlist secret/ { next }

    {
        # Split "path:line:content" while keeping colons inside the content.
        i = index($0, ":");            path = substr($0, 1, i - 1)
        rest = substr($0, i + 1)
        j = index(rest, ":");          lineno = substr(rest, 1, j - 1)
        content = substr(rest, j + 1)
    }

    # The value side of KEY=value or "KEY: value".
    {
        value = ""
        if (match(content, /[A-Za-z_][A-Za-z0-9_]*[[:space:]]*[=:][[:space:]]*/)) {
            key = substr(content, RSTART, RLENGTH)
            value = substr(content, RSTART + RLENGTH)
        }
    }

    {
        gsub(/^["\x27[:space:]]+|["\x27,[:space:]]+$/, "", value)
    }

    # Placeholders and references are exactly what belongs in a template.
    value == "" { next }
    value ~ /^\$/ { next }
    value ~ /^<.*>$/ { next }
    tolower(value) ~ /^(changeme|change-me|placeholder|example|redacted|secret|password|test|dummy|xxx+|\*+)$/ { next }
    # Self-describing placeholders: your_x_here, x_goes_here, SHOULD_BE_*, TODO.
    tolower(value) ~ /^(your|my|some)_/ { next }
    tolower(value) ~ /_here$|_goes_here$|^todo|^fixme|should_be/ { next }
    # A value equal to its own key name is a template, not a credential.
    toupper(value) == toupper(keyname(key)) { next }
    # Well-known defaults that belong to nobody.
    tolower(value) ~ /^(postgres|mysql|root|admin|guest|user|local|localhost|none|null)$/ { next }
    value ~ /^(.)\1+$/ { next }

    # 0. Credentials inside a URL. The key name says nothing here
    #    (DATABASE_URL, REDIS_URL, AMQP_URL), so nothing else catches it —
    #    this is the shape that actually leaked.
    content ~ /[a-zA-Z][a-zA-Z0-9+.-]*:\/\/[^:\/@[:space:]]+:[^@[:space:]]+@/ {
        # A ${REFERENCE} or a <placeholder> in the password position is the
        # shape we want authors to use, not a leak.
        if (content !~ /:\/\/[^:\/@[:space:]]+:[\$<]/ &&
            !looks_synthetic(url_password(content)) &&
            !is_placeholder(url_password(content))) {
            print path ":" lineno ": credentials embedded in a URL"
            next
        }
    }

    # A secret value is a single opaque token. Anything with whitespace or code
    # punctuation is a line of source, not an assignment of a credential —
    # `const MIN_SECRET_LEN: usize = 12;` must not stop a commit.
    value ~ /[[:space:]]/ { next }
    value ~ /[(){}<>;,]|::/ { next }

    # Obvious test fixtures: a real secret is not a repeated block.
    looks_synthetic(value) { next }

    # 1. A secret-named key carrying something that looks generated. Generated
    #    credentials mix digits and letters (hex, base64, alphanumeric); a
    #    descriptive fixture word like `author-value` does not.
    toupper(key) ~ keyre && length(value) >= 8 &&
    ((value ~ /[0-9]/ && value ~ /[A-Za-z]/) || length(value) >= 20) {
        print path ":" lineno ": secret-named key with a literal value"
        next
    }

    # 2. A long hex or base64 blob as a value, whatever the key is called.
    value ~ /^[0-9a-fA-F]{32,}$/ {
        print path ":" lineno ": " length(value) "-char hex value"
        next
    }
    value ~ /^[A-Za-z0-9+\/]{40,}={0,2}$/ {
        print path ":" lineno ": long base64-looking value"
    }

    # The bare key name from a "KEY=" / "KEY:" capture.
    function keyname(k) {
        gsub(/[[:space:]=:]+$/, "", k)
        return k
    }

    # Placeholder credentials: well-known defaults and self-describing stand-ins.
    function is_placeholder(v) {
        v = tolower(v)
        if (v ~ /^(postgres|mysql|root|admin|guest|user|password|changeme|secret|example|test)$/) return 1
        if (v ~ /^(your|my|some)_/) return 1
        if (v ~ /_here$|_goes_here$|should_be/) return 1
        return 0
    }

    # The credential between "//user:" and "@" of the first URL on the line.
    function url_password(line,   rest, at) {
        if (!match(line, /:\/\/[^:\/@[:space:]]+:/)) return ""
        rest = substr(line, RSTART + RLENGTH)
        at = index(rest, "@")
        return at ? substr(rest, 1, at - 1) : rest
    }

    # True for values that no random generator would produce: a repetition of a
    # shorter block (0123456789abcdef0123456789abcdef) or a single repeated
    # character. Test fixtures need secret-shaped values; real secrets are not
    # shaped like this.
    function looks_synthetic(v,   n, half, i) {
        if (v == "") return 0
        if (v ~ /^(.)\1+$/) return 1
        n = length(v)
        for (half = 1; half <= n / 2; half++) {
            if (n % half != 0) continue
            if (v == repeat(substr(v, 1, half), n / half)) return 1
        }
        return 0
    }

    function repeat(unit, times,   out, i) {
        out = ""
        for (i = 0; i < times; i++) out = out unit
        return out
    }
')

[ -n "$findings" ] || exit 0

echo "${RED}Commit refused: a concrete secret value would be committed.${RESET}" >&2
echo >&2
printf '%s\n' "$findings" | sed 's/^/  /' >&2
echo >&2
echo "${YELLOW}This repository is public — committing a value discloses it permanently," >&2
echo "and rotating afterwards does not un-publish it.${RESET}" >&2
echo >&2
echo "Use a \${REFERENCE} or a placeholder. If the line is genuinely not a secret:" >&2
echo "  append  # pragma: allowlist secret" >&2
exit 1

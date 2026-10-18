# Every check CI runs, plus a few that only help a human.
#
# CI runs these same recipes one per job, so the two cannot drift:
#
#   nix develop .#ci   --command just <recipe>
#   nix develop .#msrv --command just msrv
#
# Everything passes --locked, or `msrv` would be meaningless.

# Named explicitly because `--workspace` also selects the examples, which depend on the
# facade with default features. Cargo unifies features across the selection, so
# `--workspace --no-default-features` would have them switch the defaults back on and
# assert nothing.
libs := "-p mikrotik-proto -p mikrotik-tokio -p mikrotik-embassy -p mikrotik-rs"

# The subset that is meant to build without `std`. mikrotik-tokio is absent because it
# requires it; see `test-minimal`.
nostd_libs := "-p mikrotik-proto -p mikrotik-embassy -p mikrotik-rs"

default:
    @just --list --unsorted

# ---------------------------------------------------------------- checks

[doc('rustfmt, check only')]
fmt:
    cargo fmt --all --check

# `-D warnings` escalates the pedantic tier. After `--`, so it does not apply to dependencies.
[doc('clippy, with all features and with none')]
clippy:
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
    cargo clippy {{ libs }} --all-targets --no-default-features --locked -- -D warnings

# --cfg docsrs matches docs.rs, which the manifests opt into.
[doc('rustdoc, warnings denied, as docs.rs builds it')]
doc:
    RUSTDOCFLAGS="-D warnings --cfg docsrs" cargo doc --workspace --all-features --no-deps --locked

# nextest for the per-test timeout; it skips doctests, hence the second line.
[doc('tests and doctests; FEATURES passes through to cargo')]
test *FEATURES:
    cargo nextest run --workspace --locked --profile ci {{ FEATURES }}
    cargo test --doc --workspace --locked {{ FEATURES }}

# mikrotik-tokio is excluded deliberately: it depends on mikrotik-proto with
# `features = ["std"]`, and a dependency edge is not something --no-default-features can
# switch off. Including it unified `std` back on and made this recipe assert nothing.
[doc('tests with no default features, where --workspace would be a no-op')]
test-minimal:
    cargo nextest run {{ nostd_libs }} --locked --profile ci --no-default-features
    cargo test --doc {{ nostd_libs }} --locked --no-default-features

# A new feature extends the sweep without editing this file. The powerset is exponential,
# so it targets the facade, where the combinations live.
[doc('every feature alone, then the facade powerset')]
features:
    cargo hack clippy --each-feature --workspace --ignore-private --all-targets --locked -- -D warnings
    cargo hack clippy --feature-powerset -p mikrotik-rs --all-targets --locked -- -D warnings

# `build` for bare metal: only codegen fails on a missing lang item, and a host build with
# --no-default-features keeps std in the sysroot so it cannot fail. thumbv6m has no
# compare-and-swap, which is what stops an atomic reappearing in the protocol core.
#
# `check` is enough for the hosted targets; linking them needs their SDK.
[doc('bare-metal builds, and cross-checks for the hosted platforms')]
targets:
    cargo build -p mikrotik-proto --target thumbv6m-none-eabi --locked
    cargo build -p mikrotik-embassy --target thumbv6m-none-eabi --locked
    cargo build -p mikrotik-proto --target thumbv7em-none-eabihf --locked
    cargo build -p mikrotik-embassy --target thumbv7em-none-eabihf --locked
    cargo check --workspace --all-features --all-targets --locked --target aarch64-apple-darwin
    cargo check --workspace --all-features --all-targets --locked --target aarch64-unknown-linux-gnu

# Run in devShells.msrv: nix develop .#msrv --command just msrv
[doc('cargo check on the oldest supported compiler')]
msrv:
    rustc --version
    cargo check --workspace --all-features --all-targets --locked

# More cases than nextest's timeout tolerates, hence plain `cargo test`.
[doc('the extended property test runs')]
proptest:
    PROPTEST_CASES=8192 cargo test -p mikrotik-proto --all-features --test proptest --locked

[doc('what cargo publish would do, before release-plz does it')]
publish:
    #!/usr/bin/env bash
    set -euo pipefail
    # Catches publishing traps on the PR that introduces them, not halfway through a
    # release when some crates are already on the registry.
    #
    # The facade's README is a copy, not a symlink: release-plz diffs each package in a
    # temp directory and the relative target escapes it. Checked before the dirty-tree
    # guard, because editing the README is when the two drift.
    echo "checking that the facade readme matches the repository readme"
    if ! diff -q README.md crates/mikrotik-rs/README.md >/dev/null; then
        echo "::error::crates/mikrotik-rs/README.md has drifted from README.md"
        echo "run: cp README.md crates/mikrotik-rs/README.md"
        exit 1
    fi

    # `cargo package` refuses on a dirty tree, which only bites a developer mid-change.
    if [[ -z ${GITHUB_ACTIONS-} && -n $(git status --porcelain) ]]; then
        echo "skipping the packaging checks: the working tree is dirty."
        echo "commit or stash, or run this on a clean checkout as CI does."
        exit 0
    fi

    # Only the bottom of the graph packages for real: `cargo package` resolves against the
    # registry, and nothing above mikrotik-proto can until it is published. `--list` does
    # the manifest rewrite without resolving. `.publish == null` filters out the examples.
    root=mikrotik-proto
    cargo package -p "$root" --locked
    for crate in $(cargo metadata --no-deps --format-version 1 --locked |
        yq -p json -r '.packages[] | select(.publish == null) | .name' | sort); do
        [[ $crate == "$root" ]] && continue
        cargo package -p "$crate" --list --locked
    done

    echo "checking that the facade readme survives packaging"
    if ! cargo package -p mikrotik-rs --list --locked | grep -qx 'README.md'; then
        echo "::error::crates/mikrotik-rs/README.md is missing from the package"
        exit 1
    fi

# Policy and every exception live in deny.toml.
[doc('advisories, licences, duplicate versions, sources')]
deny:
    cargo deny check advisories bans licenses sources

# cargo-machete scans source, so a macro-only use is a false positive: record it as
# [package.metadata.cargo-machete] ignored rather than dropping the check.
[doc('dependencies that nothing actually uses')]
unused:
    cargo machete

# Catches breakage on the PR rather than in the release PR. Worth it here because the
# facade re-exports the adapters wholesale: a change to a type in -proto breaks it
# without the facade being touched.
[doc('API breaking changes against the last published version')]
semver:
    cargo semver-checks --workspace --all-features

[doc('TOML formatting')]
toml:
    taplo fmt --check --diff

# typos honours .gitignore, so it needs no exclude list.
[doc('spelling, in prose and in comments')]
typos:
    typos

[doc('actionlint')]
workflows:
    actionlint .github/workflows/*.yml

[doc('the whole suite, minus msrv, which needs its own shell')]
all: fmt clippy doc features targets proptest publish deny unused semver toml typos workflows
    @just test --all-features
    @just test
    @just test-minimal

# ---------------------------------------------------------------- development

# bacon.toml redefines bacon's jobs to match `clippy`, `test` and `doc` above, rather than its
# default bare `cargo check`.
[doc('rebuild and retest on every save')]
watch:
    bacon

[doc('apply what clippy, rustfmt and taplo can fix themselves')]
fix:
    cargo clippy --workspace --all-targets --all-features --fix --allow-dirty --allow-staged
    cargo fmt --all
    taplo fmt

[doc('build the docs and open them')]
docs:
    cargo doc --workspace --all-features --no-deps --open

[doc('dependencies with newer versions available')]
outdated:
    cargo outdated --workspace --root-deps-only

[doc('update the lockfile and the flake inputs')]
update:
    cargo update
    nix flake update

# Not from the flake and not in `all`: the hosted app runs ahead of nixpkgs, so validating
# against a pinned copy misleads. No filename argument — naming the file would validate it
# as renovate's *global* schema, which is a different thing.
[doc('validate renovate.json5 against the renovate schema')]
renovate:
    nix shell nixpkgs#renovate -c renovate-config-validator

# No `release-preview`: with release_always = false, --dry-run answers "not from a release
# PR" rather than naming the next version. The release PR itself is the preview.
[doc('what would be published, as a dry run (needs GITHUB_TOKEN)')]
release-check:
    release-plz release --dry-run --forge github \
        --git-token "${GITHUB_TOKEN:?set it, e.g. export GITHUB_TOKEN=$(gh auth token)}"

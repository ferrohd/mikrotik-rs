# Every check CI runs, and a few commands that only help a human.
#
#   just              list the recipes
#   just all          the whole check suite, as CI runs it
#
# CI runs these same recipes, one per job, so the two cannot drift:
#
#   nix develop .#ci   --command just <recipe>
#   nix develop .#msrv --command just msrv
#
# Everything passes --locked, or cargo may resolve a different version than the lockfile — which
# makes `msrv` meaningless, since a dependency raising its own minimum would fail for an
# unrelated reason. Recipes carry [doc] because `just --list` shows only a comment's last line.

# The four published crates, named explicitly.
#
# `--workspace` also selects the five `publish = false` crates under examples/, each of which
# depends on mikrotik-rs with default features. Cargo unifies features across the whole
# selection, so `--workspace --no-default-features` silently has the examples switch the facade's
# defaults back on and checks nothing at all — it passes, which is worse than failing. Every
# recipe that varies features therefore names the libraries instead.
libs := "-p mikrotik-proto -p mikrotik-tokio -p mikrotik-embassy -p mikrotik-rs"

default:
    @just --list --unsorted

# ---------------------------------------------------------------- checks

[doc('rustfmt, check only')]
fmt:
    cargo fmt --all --check

# [workspace.lints] already denies clippy::all, missing_docs and unsafe_code; `-D warnings`
# escalates the `pedantic = warn` tier. After `--` so it applies here and not to dependencies.
[doc('clippy, with all features and with none')]
clippy:
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
    cargo clippy {{ libs }} --all-targets --no-default-features --locked -- -D warnings

# --cfg docsrs matches docs.rs, which the member manifests opt into.
[doc('rustdoc, warnings denied, as docs.rs builds it')]
doc:
    RUSTDOCFLAGS="-D warnings --cfg docsrs" cargo doc --workspace --all-features --no-deps --locked

# nextest for the per-test timeout in .config/nextest.toml: `cargo test` runs a binary's tests in
# one process and cannot notice one that hangs, so a blocked test stalls the job until the
# workflow timeout kills it. Both adapter suites drive a real socket or pipe, which is the shape
# that hangs. nextest skips doctests, hence the second line.
[doc('tests and doctests; FEATURES passes through to cargo')]
test *FEATURES:
    cargo nextest run --workspace --locked --profile ci {{ FEATURES }}
    cargo test --doc --workspace --locked {{ FEATURES }}

# As `test`, but scoped to the libraries so the examples cannot re-enable the facade's defaults.
[doc('tests with no default features, where --workspace would be a no-op')]
test-minimal:
    cargo nextest run {{ libs }} --locked --profile ci --no-default-features
    cargo test --doc {{ libs }} --locked --no-default-features

# Self-maintaining: a new feature extends the sweep without editing this file. --ignore-private
# drops the examples, which have no features of their own. The powerset is exponential, so it
# targets one crate — the facade, whose five features are where the combinations actually live.
[doc('every feature alone, then the facade powerset')]
features:
    cargo hack clippy --each-feature --workspace --ignore-private --all-targets --locked -- -D warnings
    cargo hack clippy --feature-powerset -p mikrotik-rs --all-targets --locked -- -D warnings

# `check` not `build`: linking a foreign target needs its SDK, and checking already catches what
# this is for — a dependency gating an API behind #[cfg] and a call site that ignores it, an
# E0599 that testing on Linux never reveals. The list is `crossTargets` in flake.nix.
#
# Missing, and the reason this recipe is weaker than it should be:
#
#     cargo build -p mikrotik-proto   --target thumbv7em-none-eabihf --locked
#     cargo build -p mikrotik-embassy --target thumbv7em-none-eabihf --locked
#
# `build`, not `check`, because only codegen fails on a missing lang item. Both lines fail
# today: Tag wraps uuid::Uuid, uuid's `v4` feature pulls getrandom, and getrandom
# compile_error!s on a target with no OS entropy. So mikrotik-embassy — a crate that exists to
# be run on microcontrollers — has never been buildable for one, and `--no-default-features` on
# the host never showed it, because the host sysroot still has std. Uncomment both lines in the
# commit that gives Tag a no_std source.
[doc('cross-checks for the shipped platforms')]
targets:
    cargo check --workspace --all-features --all-targets --locked --target aarch64-apple-darwin
    cargo check --workspace --all-features --all-targets --locked --target aarch64-unknown-linux-gnu

# Run in devShells.msrv: nix develop .#msrv --command just msrv
[doc('cargo check on the oldest supported compiler')]
msrv:
    rustc --version
    cargo check --workspace --all-features --all-targets --locked

# Property tests need far more than the default 256 cases to be worth running, and at that size
# they fight nextest's timeout — hence plain `cargo test` and a separate job.
[doc('the extended property test runs')]
proptest:
    PROPTEST_CASES=8192 cargo test -p mikrotik-proto --all-features --test proptest --locked

[doc('what cargo publish would do, before release-plz does it')]
publish:
    #!/usr/bin/env bash
    set -euo pipefail
    # Catches publishing traps on the pull request that introduces them, rather than halfway
    # through a release when some crates are already on the registry and cannot be taken back.
    #
    # crates/mikrotik-rs/README.md is a byte-for-byte copy of the repository README, so that
    # the facade's crates.io page and the GitHub landing page say the same thing.
    #
    # A symlink was the obvious way to avoid the copy, and cargo does dereference one when
    # packaging — but release-plz copies each package into a temp directory to work out which
    # files changed, the relative target escapes what it copies, and the facade's manifest came
    # back unreadable. A real file and this assertion instead. Before the dirty-tree guard
    # below, because editing the README is exactly when the two drift.
    echo "checking that the facade readme matches the repository readme"
    if ! diff -q README.md crates/mikrotik-rs/README.md >/dev/null; then
        echo "::error::crates/mikrotik-rs/README.md has drifted from README.md"
        echo "run: cp README.md crates/mikrotik-rs/README.md"
        exit 1
    fi

    # `cargo package` refuses to describe a package with uncommitted changes — rightly, since
    # the answer would not be reproducible. Only bites a developer running `all` mid-change.
    if [[ -z ${GITHUB_ACTIONS-} && -n $(git status --porcelain) ]]; then
        echo "skipping the packaging checks: the working tree is dirty."
        echo "commit or stash, or run this on a clean checkout as CI does."
        exit 0
    fi

    # Only the bottom of the graph can be packaged for real: `cargo package` resolves against
    # the registry, and nothing above mikrotik-proto can resolve until the release that
    # publishes it lands. `--list` does the manifest rewrite without resolving.
    #
    # The examples are filtered out by `.publish == null`, which cargo metadata uses for
    # "publishable"; `publish = false` reports an empty list. Packaging one fails outright.
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

# An unused dependency is still resolved, locked, audited and counted against the
# duplicate-version policy. cargo-machete scans source, so macro-only uses are false positives:
# record them as [package.metadata.cargo-machete] ignored = [...] rather than dropping the check.
[doc('dependencies that nothing actually uses')]
unused:
    cargo machete

# Unlike a project that has never released, this one has four crates on the registry, so the
# baseline exists and breakage is caught on the pull request rather than in the release PR once
# it is already on master. Worth the job here in particular: mikrotik-rs re-exports the adapters
# wholesale, so a change to a type in -proto is a facade break without touching the facade.
# release-plz runs this itself at release time regardless — it is how it decides to bump a major.
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

# Not from the flake, and not in `all`: renovate.json5 is read by the Mend-hosted app, which runs
# a version ahead of nixpkgs, so validating against a pinned older copy misleads. Nothing carries
# its 269 MiB closure otherwise.
#
# No filename argument — naming the file validates it as renovate's *global* config schema, which
# is a different and more permissive thing. With no arguments it discovers and validates the file
# as a repository config.
[doc('validate renovate.json5 against the renovate schema')]
renovate:
    nix shell nixpkgs#renovate -c renovate-config-validator

# No `release-preview`: only `release` takes --dry-run, and with release_always = false it answers
# "not from a release PR" rather than naming the next version. The real preview is the release
# pull request release-plz opens on every push to master.
[doc('what would be published, as a dry run (needs GITHUB_TOKEN)')]
release-check:
    release-plz release --dry-run --forge github \
        --git-token "${GITHUB_TOKEN:?set it, e.g. export GITHUB_TOKEN=$(gh auth token)}"

# Contributing to mikrotik-rs 🤝

First off, thank you for considering contributing to mikrotik-rs! Welcome contributions from everyone as long as they adhere to our guidelines.

## Code of Conduct

mikrotik-rs adheres to a Code of Conduct that we expect project participants to adhere to. Please read [the full text](CODE_OF_CONDUCT.md) so that you can understand what actions will and will not be tolerated.

## Development setup

The project pins its toolchain and every check tool with a Nix flake, so that what you run
locally and what CI runs are the same binaries.

```bash
nix develop          # or `direnv allow`, which does it on cd
just                 # list every recipe
just all             # the whole check suite, exactly as CI runs it
just watch           # rebuild and retest on every save
just fix             # apply what clippy, rustfmt and taplo can fix themselves
```

CI runs one job per recipe, as `nix develop .#ci --command just <recipe>`, so a green `just all`
means a green pull request. There is one exception, because it needs its own compiler:

```bash
nix develop .#msrv --command just msrv
```

**Without Nix**, the recipes are ordinary cargo commands and will work once you have the tools
on `PATH`: a stable toolchain plus `just`, `cargo-nextest`, `cargo-hack`, `cargo-deny`,
`cargo-semver-checks`, `cargo-machete`, `taplo`, `typos`, `actionlint` and `yq`. Versions are
then yours to manage, and `just all` may disagree with CI about a lint.

Useful to know:

- `cargo build` alone is a weaker gate than CI. `[workspace.lints]` forbids `unsafe_code`,
  denies `missing_docs` and `clippy::all`, and CI escalates `clippy::pedantic` with
  `-D warnings`.
- The MSRV is declared once, as `rust-version` in the root `Cargo.toml`. `flake.nix` reads it,
  so raising it is a one-line change — but it is a user-visible one, so say so in the PR.
- New dependencies have to pass `cargo deny`: an allowed licence, no duplicate versions in the
  graph, and no advisories. See [`deny.toml`](deny.toml).
- Public items need documentation. `missing_docs` is denied, not warned.

See [docs/architecture.md](docs/architecture.md) for how the crates fit together.

## Commit and pull request titles

The changelog and the version bumps are generated from commit messages, so the format is load
bearing rather than a style preference. Pull requests are rebase-merged, so **every commit**
lands on `master` as written and every commit subject feeds the changelog. CI additionally
checks the pull request title, since that is what is used if a PR is squashed instead.

Keep a branch to commits you are happy to publish — `just fix`-then-amend is cheaper than
explaining a "wip" entry in a release note.

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add a query operator for regex matching
fix(proto): reject a sentence whose length prefix overruns the buffer
docs: explain the tag lifecycle
```

Accepted types, and where each one lands in the changelog:

| Type | Changelog section | Version effect |
|---|---|---|
| `feat` | Added | minor |
| `fix` | Fixed | patch |
| `perf` | Performance | patch |
| `refactor`, `revert` | Changed | patch |
| `docs` | Documentation | patch |
| `chore(deps)`, `build(deps)` | Dependencies | patch |
| `ci`, `style`, `test`, `chore`, `build` | omitted | none |

A breaking change is marked either with a `!` after the type (`feat!:`) or a
`BREAKING CHANGE:` trailer, and is never omitted from the changelog regardless of type.

Releases are handled by [release-plz](https://release-plz.dev): merging to `master` opens or
updates a release pull request carrying the version bump and the changelog, and merging *that*
publishes all four crates. Do not bump versions or edit `CHANGELOG.md` by hand.

## How Can I Contribute?

Contributions to mikrotik-rs can be made in several ways, not just by modifying the project's code. Below are some ways you can contribute:

### Reporting Bugs 🐛

This section guides you through submitting a bug report for mikrotik-rs. Following these guidelines helps maintainers and the community understand your report 📝, reproduce the behavior 💻, and find related reports 🕵️‍♂️.

**Before Submitting A Bug Report**

- Check the issue yourself - you might be able to find the cause of the problem and fix it yourself.
- Determine which package/crate the problem is related to.
- Perform a cursory search to see if the bug has already been reported. If it has **and the issue is still open**, add a comment to the existing issue instead of opening a new one.

**How Do I Submit A (Good) Bug Report?**

- Use a clear and descriptive title for the issue to identify the problem.
- Describe the exact steps which reproduce the problem in as much detail as possible.
- Provide specific examples to demonstrate the steps.
- Describe the behavior you observed after following the steps and point out what exactly is the problem with that behavior.
- Explain which behavior you expected to see instead and why.
- Include (possibly) screenshots and console logs which demonstrate the problem.

### Suggesting Enhancements ✨

This section guides you through submitting an enhancement suggestion for mikrotik-rs, including completely new features and minor improvements to existing functionality. Following these guidelines helps maintainers and the community understand your suggestion 📢 and find related suggestions 🕵️‍♂️.

**Before Submitting An Enhancement Suggestion**

- Perform a cursory search to see if the enhancement has already been suggested. If it has, add a 👍 to the existing suggestion.

**How Do I Submit A (Good) Enhancement Suggestion?**

- Use a clear and descriptive title for the issue to identify the suggestion.
- Provide a step-by-step description of the suggested enhancement in as many details as possible.
- Provide specific examples to demonstrate the steps. Include copy-pasteable snippets which you use in those examples, as Markdown code blocks.
- Describe the current behavior and explain which behavior you expected to see instead and why.
- Explain why this enhancement would be useful to most mikrotik-rs users.
- List some other projects where this enhancement exists if possible.

### Pull Requests 📦

The process for submitting a pull request is as follows:

- Fork the repository to your own GitHub account.
- Clone the repository to your local device.
- Create a new branch for your fix or feature.
- Make the changes on your new branch.
- Push your branch to your fork on GitHub.
- From your fork, open a new pull request in the correct branch.
- Once the pull request is approved and merged, you can pull the changes from upstream to your local repo and delete your extra branch(es).

**Before Submitting A Pull Request**

- Run `just all`. It is the same suite CI runs, so a failure here is a failure there.
- Give the pull request, and every commit in it, a Conventional Commits subject — see above.
  They land on `master` as written and feed the changelog.
- Add or update the documentation as needed. `missing_docs` is denied, so a new public item
  without a doc comment will not compile.
- Submit your pull request with a comprehensive description of your changes.

### Reporting a security issue

Do not open a public issue or pull request for a vulnerability. Follow
[SECURITY.md](SECURITY.md) instead.

## Contributor License Agreement

By submitting a pull request, you agree that:

1. Your contributions are licensed under the AGPL-3.0-only license.
2. You grant the project maintainer (Alessandro Ferrara) a perpetual, worldwide, non-exclusive, royalty-free license to use, reproduce, modify, and distribute your contributions under any other license, including a commercial license.

This is necessary to support the project's dual-licensing model.

## Questions?

Don't hesitate to ask! Open an issue and tag it as a question.

Once again, thanks for your interest in contributing to mikrotik-rs. We look forward to your contributions!

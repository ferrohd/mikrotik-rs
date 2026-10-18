# Security Policy

## Supported versions

Only the latest published version receives security fixes. The four crates in this workspace
share a single version and are released together, so a fix ships to all of them at once.

| Version | Supported |
|---------|-----------|
| 0.9.x   | yes       |
| < 0.9   | no        |

This library is pre-1.0. Upgrading to collect a security fix may require code changes.

## Reporting a vulnerability

**Do not open a public issue.**

Report privately through GitHub's
[security advisory form](https://github.com/ferrohd/mikrotik-rs/security/advisories/new), which
opens a channel visible only to you and the maintainer.

Please include the affected crate and version, what an attacker can achieve, and a reproduction
if you have one. You should get an acknowledgement within a week.

If a report is accepted, the fix is prepared in a private fork, released, and published as a
GitHub Security Advisory with a RustSec identifier requested, so that `cargo deny check
advisories` and `cargo audit` pick it up for everyone else. Reporters are credited unless they
ask otherwise.

## Scope

This is a protocol client. The things most worth looking at:

- **Parsing.** `mikrotik-proto` decodes whatever bytes the peer sends. A panic, a hang or an
  unbounded allocation reachable from router-controlled input is a vulnerability, not a bug.
  The parser is property-tested against arbitrary input for exactly this reason.
- **TLS.** The `tokio-tls` feature includes a `tls_insecure()` builder method that disables
  certificate verification. This is deliberate — MikroTik routers ship self-signed certificates
  — and it is documented as insecure. A path that skips verification *without* the caller
  asking for it is in scope.
- **Credentials.** Anything that writes a password to a log, an error message or a `Debug`
  output.

Out of scope: the security of RouterOS itself, and the consequences of deliberately calling
`tls_insecure()`.

## Dependencies

The dependency graph is checked on every pull request and weekly against the
[RustSec advisory database](https://rustsec.org/) by `cargo deny`, configured in
[`deny.toml`](deny.toml). Advisories against dependencies are usually fixed by a dependency
bump rather than by a report here.

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.10.0](https://github.com/ferrohd/mikrotik-rs/compare/mikrotik-rs-v0.9.0...mikrotik-rs-v0.10.0) - 2026-10-23

### Changed

- *(proto)* describe outbound words instead of encoding them
- restructure the workspace and unify crate versions

### Documentation

- cut architecture.md back to structure and testing
- *(tokio,embassy,rs)* trim module docs, fix stale versions
- record the tag and attribute encoding models
- switch to renovate.json5, document the workflow, add a policy

### Fixed

- *(proto)* [**breaking**] omit attribute words whose value is None

## [0.9.0] - 2026-09-09

### Documentation

- ship the AGPL text with every published crate

### Other

- use unbounded channels

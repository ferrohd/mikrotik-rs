# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.10.0](https://github.com/ferrohd/mikrotik-rs/compare/mikrotik-proto-v0.2.1...mikrotik-proto-v0.10.0) - 2026-10-23

### Added

- *(proto)* [**breaking**] reject a command whose tag is already in flight
- *(proto)* gate the global tag counter on the std feature

### Changed

- *(proto)* [**breaking**] replace UUID command tags with a u32 counter
- *(proto)* describe outbound words instead of encoding them
- restructure the workspace and unify crate versions

### Documentation

- *(proto)* trim comments to what prevents a regression
- record the tag and attribute encoding models
- switch to renovate.json5, document the workflow, add a policy

### Fixed

- *(proto)* [**breaking**] omit attribute words whose value is None

## [0.2.1] - 2026-09-09

### Changed

- *(proto)* split CommandResponse::parse into per-category helpers
- *(proto)* centralize usize to u32 word-length narrowing

### Documentation

- ship the AGPL text with every published crate
- correct stale MIT/Apache license notice in sub-crate READMEs

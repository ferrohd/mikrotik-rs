# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.10.0](https://github.com/ferrohd/mikrotik-rs/compare/mikrotik-embassy-v0.1.2...mikrotik-embassy-v0.10.0) - 2026-10-23

### Added

- *(embassy)* add a Client handle for the command channel
- *(proto)* gate the global tag counter on the std feature

### Changed

- restructure the workspace and unify crate versions

### Documentation

- *(tokio,embassy,rs)* trim module docs, fix stale versions
- switch to renovate.json5, document the workflow, add a policy

## [0.1.2] - 2026-09-09

### Changed

- *(embassy)* inline map_io at its call sites

### Documentation

- ship the AGPL text with every published crate
- correct stale MIT/Apache license notice in sub-crate READMEs

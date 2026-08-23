# Changelog

All notable changes to Once are documented in this file. The project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Load policy from an immutable external GitHub flake, verify its full revision
  through Nix metadata, and fail closed before target evaluation on any
  mismatch.
- Provide a separately publishable policy-flake template and document its
  protected update and invocation boundaries.
- Package `doctor`, `check`, and `run` as a pinned composite GitHub Action with
  versioned JSON output, exact policy exit codes, and fail-closed defaults.
- Give `resolve`, `trace`, and `trust` distinct versioned diagnostic views and
  expand human-readable `explain` output while preserving policy exit codes.
- Lock every public v1 JSON shape with golden fixtures and cover fail-closed,
  no-target-build diagnostic behavior across ambiguous trace states.

### Fixed

- Generate release checksum files with portable archive basenames so
  `sha256sum -c` works after downloading the assets into any directory.

## [0.1.1] - 2026-08-23

### Changed

- Replace the abbreviated license notice with the canonical Apache-2.0 text
  and record Closure Labs and Dale Morgan as copyright holders.
- Document installation from a pinned GitHub release.
- Package tagged releases as checksummed Linux archives and publish their
  flakes to FlakeHub.
- Record the successful v0.1.0 acceptance run.

## [0.1.0] - 2026-08-23

### Added

- A Rust CLI with `doctor`, `check`, and `run` commands, stable JSON output,
  explicit exit codes, and GitHub Actions summaries.
- Native Nix 2.35 build-trace inspection without intentionally building the
  requested installable during `once check`.
- Fail-closed local trust policy for signer identity, input-addressed modes,
  conflicting traces, unsupported environments, and malformed configuration.
- A content-addressed Nix proof of concept and disposable-store demonstration
  covering cold misses, accepted repeat checks, artifact deletion, relevant and
  irrelevant input changes, and policy-version invalidation.
- Architecture, threat-model, implementation, acceptance, and Nix 2.35
  investigation documentation.
- CI checks for formatting, linting, tests, the flake, and the full proof of
  concept demonstration.

### Known limitations

- Nix 2.35's remote build-trace inspection does not validate returned
  signatures. Once therefore accepts trace reuse only from a trusted local
  store and reports remote trace hits as `UNSUPPORTED`.
- The v0.1 proof of concept supports `x86_64-linux`; durable remote backends,
  threshold signatures, and additional platforms remain future work.

[0.1.0]: https://github.com/closure-labs/once/releases/tag/v0.1.0
[0.1.1]: https://github.com/closure-labs/once/compare/v0.1.0...v0.1.1
[Unreleased]: https://github.com/closure-labs/once/compare/v0.1.1...HEAD

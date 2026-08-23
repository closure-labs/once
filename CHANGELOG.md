# Changelog

All notable changes to Once are documented in this file. The project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.1] - 2026-08-23

### Added

- Add a pinned `default.nix` entry point for conventional `nix-build` use from
  a repository checkout.

### Changed

- Make the Closure Labs GitHub repository the documented package source for
  both conventional Nix and flake consumers during Once's early-maturity phase.
- Record official Nixpkgs submission as a long-term milestone after the package
  interfaces and security behavior have matured.

## [0.4.0] - 2026-08-23

### Added

- Export Once as a repository-hosted Nix package through a reusable Nixpkgs
  overlay and standalone `callPackage`-compatible package expression.
- Verify the installed package reports the release version during Nix builds.

### Changed

- Source the development package set directly from the community Nixpkgs
  upstream instead of the FlakeHub mirror.

## [0.3.0] - 2026-08-23

### Added

- Add a full-revision-pinned Nix 2.35.2/2.36 prerelease build-trace matrix
  using ephemeral keys, isolated stores, loopback HTTP, and accepted- versus
  unrelated-key substitution controls.
- Add the `dev.closurelabs.det/v1` deterministic signed-JSON proof of concept,
  including Ed25519 key generation, matrix artifact creation, pinned-key
  verification, and negative security tests.

### Security

- Confirm empirically that the pinned 2.36 prerelease rejects an unrelated-key
  build trace during substitution, while read-only trace inspection remains
  unverified and Once continues to fail closed as `UNSUPPORTED`.

## [0.2.0] - 2026-08-23

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
- Verify the minimum supported Nix release through a full-commit-pinned
  community nixpkgs `nixVersions.latest` compatibility check in CI.

### Fixed

- Generate release checksum files with portable archive basenames so
  `sha256sum -c` works after downloading the assets into any directory.
- Parse Nix upstream prerelease snapshot versions such as
  `2.36.0pre20260822_88b09c6` without weakening minimum-version comparisons.

### Security

- Re-audit Nix 2.35.2 and current upstream remote build-trace validation. No
  public read-only CLI or stable C API meets Once's trust boundary, so remote
  trace hits remain fail-closed as `UNSUPPORTED`.

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
[0.2.0]: https://github.com/closure-labs/once/compare/v0.1.1...v0.2.0
[0.3.0]: https://github.com/closure-labs/once/compare/v0.2.0...v0.3.0
[0.4.0]: https://github.com/closure-labs/once/compare/v0.3.0...v0.4.0
[0.4.1]: https://github.com/closure-labs/once/compare/v0.4.0...v0.4.1
[Unreleased]: https://github.com/closure-labs/once/compare/v0.4.1...HEAD

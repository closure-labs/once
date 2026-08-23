# Once v0.1 acceptance matrix

| Requirement | Evidence | Status |
| --- | --- | --- |
| Nix 2.35 minimum and feature diagnosis | Version unit tests and `once doctor` | Implemented |
| Deterministic expensive CA derivation | Flake check and demo log | Implemented |
| Tiny dependent CA Once check | Size assertion and build trace | Implemented |
| Cold miss, build, accepted repeat | `scripts/demo.sh` | Implemented |
| Reuse after deleting expensive output | Disposable-store assertion | Implemented locally |
| Relevant input invalidates | Demo fixture mutation | Implemented |
| Irrelevant docs do not invalidate | Demo fixture mutation | Implemented |
| Policy version invalidates | Demo fixture mutation | Implemented |
| Untrusted and conflicting traces fail closed | Integration and fixture tests | Implemented |
| Human and JSON CI output | CLI integration tests | Implemented |
| GitHub Actions demonstration | [successful `main` run](https://github.com/closure-labs/once/actions/runs/32626361066) | Implemented |
| Remote signature trust without output download | [Pinned 2.35.2/2.36 matrix](nix-2.35-2.36-build-trace-proof.md) | Blocked: inspector does not validate signatures |
| Deterministic signed substitution evidence | `.det` determinism, tamper, unrelated-key, schema, and permission tests | Implemented as a research artifact |

Statuses are updated only when the corresponding automated evidence exists.

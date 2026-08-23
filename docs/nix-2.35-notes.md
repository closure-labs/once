# Nix 2.35 assumptions

Once depends on the Nix 2.35 build-trace model:

- authoritative entries are keyed by resolved derivation store path and output;
- only base entries are durable;
- native binary caches store them under `build-trace-v2`;
- signatures use structured `keyName` and `sig` fields;
- `nix store build-trace info --json` is experimental;
- content-addressed derivations remain experimental.

The 2.35.2 CLI returns both `{ "key": ..., "value": ... }` and
`{ "opaquePath": ... }` records. A missing trace is a nonzero diagnostic, not
an empty list. Cache JSON paths may omit `/nix/store/`; the adapter normalizes
them only for display and comparison.

`nix derivation show` emits JSON format 4 with a top-level `derivations` map;
its derivation keys may also be store-path basenames.

The local probe confirmed that deleting a CA output does not delete its base
trace. Signed cache publication must use a derivation-output installable such as
`/nix/store/...drv^out`; copying the opaque output path alone does not publish
the build trace.

The [remote build-trace trust audit](planning/remote-trust-audit.md) found no
public Nix 2.35.2 interface that validates a remote realisation signature while
remaining read-only. Remote trace hits therefore remain `UNSUPPORTED`.

# Nix 2.35 build-trace spike

## Environment

The initial probe ran with Determinate Nix 3.22.2, reporting upstream Nix
2.35.2. The repository's FlakeHub lock still exposes Nix 2.34.8 as the nixpkgs
package, so the development shell deliberately does not shadow the required
host Nix.

The host daemon did not have `ca-derivations` enabled. The probe therefore used
the installed 2.35.2 executable with a disposable `local?root=...` store and the
features `nix-command flakes ca-derivations`.

## Observed interface

Building a CA derivation reported an unresolved derivation from `nix build
--json`, while the build itself ran a different resolved derivation. Querying
`nix store build-trace info --json` returned heterogeneous records:

```json
[
  {
    "key": {
      "drvPath": "1kzc...-once-planning-probe.drv",
      "outputName": "out"
    },
    "value": {
      "outPath": "9337...-once-planning-probe",
      "signatures": []
    }
  },
  { "opaquePath": "/nix/store/9337...-once-planning-probe" }
]
```

A cold query exited with status 1 and a diagnostic saying Nix could not operate
on the unbuilt output. It did not return an empty JSON array. Adapters must
classify only recognized cold-miss diagnostics as `MISS`; other failures remain
fail-closed.

After deleting the realized output with `nix store delete`, the same query still
returned the base build-trace entry and did not recreate the output. This
confirms the core local deletion/reuse premise. The trace had no signature, so
it does not yet establish the signed remote-cache acceptance case.

`nix derivation show` uses JSON format 4 in this environment: the top-level
object contains `version` and a `derivations` map whose keys are store-path
basenames. Older flat maps remain accepted at the adapter boundary.

## Remaining feasibility gate

The end-to-end harness must generate an ephemeral key, publish to a native file
binary cache, and consume the trace from a clean store using only the public
key. If the public CLI cannot establish signature trust without downloading the
large output, the implementation must use a narrowly scoped upstream Nix API or
report `UNSUPPORTED`; it must not verify signatures itself.

## Remote trust result

Publishing a derivation-output installable writes the expected signed
`build-trace-v2/<resolved-drv>/out.doi`. Querying that file cache with
`nix store build-trace info`, however, returned the entry both with the correct
public key and with an unrelated ephemeral public key. The command exposes
signature metadata but does not validate it.

Once therefore accepts signed entries only when querying a local store, which
is itself the trusted registration boundary. A non-local store result is
`UNSUPPORTED` even if its signer name is configured. Remote skipping remains
blocked until an upstream CLI/C API can validate a trace without fetching the
large output.

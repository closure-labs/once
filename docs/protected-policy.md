# Protected policy flake

Once v0.2 can load trust configuration from a separately protected GitHub
flake. This prevents a candidate change from silently editing `.once.toml` and
weakening the accepted signer or IA policy used by protected CI.

## Consumer contract

The initial contract deliberately accepts only an immutable reference:

```console
revision=5a4234004dc9948a1d45d462f947b10411d91a33
once \
  --policy-flake "github:closure-labs/once-policy/$revision" \
  --policy-revision "$revision" \
  check .#checks.x86_64-linux.integration
```

The flake must export `packages.x86_64-linux.policy` as a TOML file conforming
to Once configuration schema 1. Once performs these steps before inspecting the
requested installable:

1. reject non-GitHub, mutable, abbreviated, or mismatched references;
2. ask Nix for the locked flake metadata;
3. require the reported revision to equal the expected full commit;
4. build only the tiny `#policy` output;
5. read it through `nix store cat` and apply normal schema validation.

Any ambiguity fails before target evaluation with the malformed-policy exit
code. `once check` still never builds the requested check or its expensive
target, although it may realize the separately pinned policy text object.

## Protection and update flow

The authoritative `closure-labs/once-policy` repository should require pull
requests, green flake checks, resolved review threads, and code-owner review
once Closure Labs has a second maintainer. Force pushes and branch deletion
must remain disabled.

To update policy:

1. change `policy.toml` and, when semantics change, `lib.policyVersion` in the
   policy repository;
2. merge the policy PR through its protected branch;
3. record the resulting full commit;
4. update the protected caller's flake reference and expected revision in one
   reviewed change;
5. require CI to demonstrate the expected invalidation before adoption.

Tags and branch names are not sufficient pins for the v0.2 contract.

## Remaining boundary

The external flake protects policy contents, not invocation. The workflow or
reusable Action that supplies `--policy-flake` and `--policy-revision` must also
be protected. Candidate code can still alter the derivation being checked, so
repository review remains necessary. This is memoized trust, not an independent
proof that candidate computation occurred.

Local `--config .once.toml` remains supported for development and isolated
tests, but it is not appropriate as the authoritative policy in protected CI.

The bootstrap policy repository and its active ruleset are available at
[`closure-labs/once-policy`](https://github.com/closure-labs/once-policy). Once
CI continuously resolves the commit shown above and runs `once doctor` from the
materialized policy artifact.

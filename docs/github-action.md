# GitHub Action

Once v0.2 provides a composite Action for `doctor`, `check`, and `run`. It
installs a pinned Determinate Nix release by default, builds Once from the
Action's own immutable source revision, materializes a separately pinned policy
flake, and exposes the CLI result without changing its policy semantics.

```yaml
- name: Check for an accepted realization
  id: once
  uses: closure-labs/once@<full-40-character-action-commit>
  with:
    command: check
    installable: .#checks.x86_64-linux.integration
    policy-flake: github:closure-labs/once-policy/5a4234004dc9948a1d45d462f947b10411d91a33
    policy-revision: 5a4234004dc9948a1d45d462f947b10411d91a33

- name: Consume the exact policy result
  if: always()
  env:
    ONCE_EXIT_CODE: ${{ steps.once.outputs.exit-code }}
    ONCE_RESULT_JSON: ${{ steps.once.outputs.result-json }}
  run: |
    printf 'Once exit code: %s\n' "$ONCE_EXIT_CODE"
    printf '%s\n' "$ONCE_RESULT_JSON"
```

Pin both the Action and policy by full commit. A release tag is convenient but
does not protect a high-trust workflow against a moved tag.

## Inputs and outputs

`policy-flake` and `policy-revision` are always required and retain the
immutable policy contract. `installable` is required for `check` and `run` and
must be omitted for `doctor`. `install-nix` defaults to `true`; set it to
`false` only when the job already installed a compatible Nix. The Action
accepts no arbitrary Once command or additional shell arguments.

The Action exposes `result-json`, `schema`, and the exact numeric `exit-code`.
The schema is `dev.closurelabs.once/doctor/v1` for `doctor` and
`dev.closurelabs.once/result/v1` for `check` and `run`. A configuration or
execution error can have an empty `result-json`; its exit code remains
available.

By default, every nonzero Once exit fails the Action after its outputs are
recorded. Set `fail-on-nonzero: false` when the workflow intentionally branches
on `MISS` or another decision, then inspect `exit-code` and the versioned JSON.
The Action and CLI append compact status information to the job summary.

## Untrusted pull requests

Use `check`, not `run`, when candidate code must remain evaluation-only. Keep
the workflow definition, Action commit, policy commit, accepted Nix keys, and
credentials outside candidate control. Never combine `pull_request_target`, a
checkout of untrusted candidate code, and `command: run`: that grants candidate
build logic the target branch's privileges. Do not pass secrets to a job that
evaluates an untrusted flake.

The Action protects how Once is invoked only when its caller is protected. It
does not turn a candidate-defined derivation into trusted code and it does not
expand the remote-trace trust boundary described in the threat model.

# Closure Labs — Once
## Codex implementation brief for an initial proof of concept

**Repository:** `closure-labs/once`  
**Organization:** Closure Labs  
**CLI/binary:** `once`  
**License:** Apache-2.0  
**Minimum Nix version:** upstream CppNix 2.35.0  
**Primary target platform for the POC:** `x86_64-linux` on GitHub Actions  
**Status:** Experimental proof of concept; do not describe it as a trustless proof-of-computation system.

## Product identity

The project is **Closure Labs — Once**.

Canonical proposition:

> **Build once. Recognize it thereafter.**

The name is intentionally restrained. Once does not claim "Proof", "Truth", "Verified", "Deterministic Proof", or any other stronger property than the underlying mechanism provides. It says only that, under configured trust policy, an accepted realization corresponding to the same fully resolved Nix derivation has already been recorded.

The intended repository and binary are:

```text
github.com/closure-labs/once
once
```

Canonical CLI feel:

```console
$ once check .#integration-tests
resolved:  /nix/store/...-integration-tests.drv
output:    out
trace:     found
signer:    ci.closurelabs.dev-1
accepted:  yes
SKIP
```

This should read naturally in documentation and CI:

```text
The Once check passed.
The Once check missed.
Once recognized a prior accepted realization.
Once could not establish trust, so CI rebuilt.
```

Use **Once** as the product name and `once` as the executable everywhere user-facing.

### Naming guardrails

Do not rename the product to names that overstate the security property, including variants built around:

```text
Proof
Truth
Verified
Deterministic Proof
Proof of Build
Proof of Computation
```

**Tracebound** is a valid alternate brand if Closure Labs later wants a more technical name. Its intended connotations are: native Nix build trace; binding to an exact resolved `.drv^output`; and constraint by a trust policy. It is **not** the primary name for this POC and should not appear in user-facing implementation unless explicitly requested.

---

# 1. Mission

Build **Once**, an open-source Closure Labs proof of concept that demonstrates a CI optimization and recognition pattern using **Nix 2.35+ content-addressed derivations and native build traces**.

The system should allow CI to answer:

> Has a trusted builder already produced an accepted realization for the fully resolved derivation corresponding to this Once check?

If yes, Once may tell CI to skip the expensive build/test operation.

If no, or if trust/resolution is ambiguous, Once must fail closed and require the expensive operation.

The project must use Nix's own derivation identities, resolution rules, build traces, and signature/trust machinery wherever possible. Do not invent a parallel hash graph, receipt format, or signature protocol.

This POC is intended to explore the design, ergonomics, and security boundaries of this idea; it is not intended to claim stronger guarantees than Nix build traces actually provide.

---

# 2. Core terminology and security statement

Use the following language consistently in code and documentation.

## 2.1 Build trace

A Nix build trace is a memoization table mapping **resolved derivation outputs** to realized store objects.

For Nix 2.35+, authoritative/shared build-trace entries are keyed by:

```text
<resolved .drv store path> ^ <output name>
```

and map to an output store path plus signatures.

The binary-cache namespace is:

```text
build-trace-v2/<drvBaseName>/<outputName>.doi
```

Do not implement or depend on the pre-2.35 `realisations/` format.

## 2.2 Base trace only

Treat **base build-trace entries keyed by resolved derivations** as authoritative.

Do not create an independent durable attestation for unresolved derivations.

If the implementation memoizes unresolved-to-resolved mappings for speed, treat those mappings as disposable local cache only. They must be reconstructable from base entries.

## 2.3 What a successful Once check means

A successful Once check means:

> Under the configured trust policy, Nix can resolve the current installable to a resolved derivation and locate an accepted signed build-trace entry for that resolved derivation/output.

It does **not** mean:

> A trustless cryptographic proof exists that a CPU definitely executed the original build.

Document this distinction prominently.

A build-trace entry is a signed claim. Ultimately, independently auditing the claim requires rebuilding. Nondeterministic derivations can legitimately produce different realizations.

## 2.4 Content-addressed versus input-addressed inputs

The strongest mode is when check-critical transitive inputs are content-addressed.

Input-addressed paths remain a weaker point because their path identity does not provide the same direct content identity.

The POC must report this distinction rather than hiding it.

---

# 3. POC goals

The initial release must demonstrate all of the following:

1. Require Nix >= 2.35.0.
2. Require:
   - `nix-command`
   - `flakes`
   - `ca-derivations`
3. Define a deterministic, deliberately slow content-addressed demo derivation.
4. Define a second tiny content-addressed **Once check derivation** that depends on the slow derivation but emits only a tiny check payload.
5. Build the Once check once.
6. Show that Nix records native build-trace entries for the CA derivations.
7. Allow the large/slow realized output to be removed while retaining build-trace state.
8. Re-evaluate the same Once check and detect that the trusted trace is sufficient to skip the slow operation.
9. Modify a build-relevant input and demonstrate that the derivation identity changes and the old trace no longer satisfies the Once check.
10. Modify an irrelevant file, such as documentation excluded from the derivation inputs, and demonstrate that the Once check remains reusable.
11. Change a Once policy version and demonstrate that the Once check invalidates even when the underlying product source does not.
12. Detect or surface incompatible/nondeterministic realization conflicts rather than silently choosing one.
13. Emit useful machine-readable and human-readable CI output.
14. Run the demonstration automatically in GitHub Actions.

---

# 4. Explicit non-goals for v0.1

Do not spend the initial POC implementing these:

- a new cryptographic signature scheme;
- a custom Merkle DAG;
- SLSA or Sigstore integration;
- a production Cachix integration;
- a hosted API service;
- a web UI;
- multi-platform support beyond making the architecture portable;
- automatic cloud infrastructure provisioning;
- a custom Nix daemon fork;
- full support for dynamic derivations;
- a claim that all input-addressed dependencies are cryptographically equivalent to CA dependencies;
- untrusted-fork GitHub Actions publishing with privileged credentials.

Design interfaces so these can be added later.

---

# 5. Project architecture

Implement three conceptual layers.

```text
                          once
                           |
          +----------------+----------------+
          |                |                |
          v                v                v
      Nix adapter      policy engine     reporter
          |
          v
    upstream Nix 2.35+
          |
          +---- evaluation / derivations
          +---- resolution
          +---- build trace
          +---- signature/trust enforcement
          +---- store / binary-cache protocol
```

## 5.1 Nix adapter

The Nix adapter is responsible for invoking and interpreting upstream Nix.

Prefer shelling out to the Nix 2.35+ CLI for the POC rather than linking directly against unstable C++ internals.

The adapter must:

- detect and parse `nix --version`;
- reject versions older than 2.35.0;
- verify required experimental features are usable;
- evaluate an installable without accidentally building it;
- inspect derivation/build-trace state;
- ask Nix to resolve/query the build trace;
- run a build only when instructed;
- distinguish a trace hit from a build/substitution that would download the large output;
- never infer trust merely by checking that a signer name string appears in JSON.

Observed Nix 2.35.2 behavior should shape the adapter:

- `nix store build-trace info --json` returns heterogeneous base-trace and
  `opaquePath` records;
- cache-style derivation and output paths may be basenames rather than absolute
  store paths and must be normalized at the adapter boundary;
- an unbuilt output produces a non-zero diagnostic rather than an empty JSON
  result, so only known cold-miss diagnostics may become `MISS`;
- unknown diagnostics must remain `ERROR`.

Use Nix itself as the trust oracle wherever possible.

If the Nix CLI does not expose enough information to verify trust safely, the implementation may use the Nix C API/libstore as a narrowly scoped fallback. Do not implement Nix's signature verification algorithm from scratch in v0.1.

## 5.2 Policy engine

The policy engine decides whether an available trace is sufficient for an Once check to recognize the prior realization and skip the expensive operation.

Minimum Once trust-policy checks:

- minimum Nix version;
- accepted builder/signing keys;
- required signature count, initially 1;
- Once policy version;
- IA-input handling mode;
- trace conflict handling;
- fail-closed behavior for unsupported/unknown states.

Possible result classes:

```text
ACCEPTED_CA
ACCEPTED_WITH_IA_TRUST
MISS
UNTRUSTED
CONFLICT
UNSUPPORTED
ERROR
```

Only `ACCEPTED_CA` and, when policy permits, `ACCEPTED_WITH_IA_TRUST` may skip work.

## 5.3 Reporter

Provide:

- concise terminal output;
- `--json`;
- GitHub Actions summary output when `GITHUB_STEP_SUMMARY` exists;
- stable exit codes.

Suggested exit codes:

```text
0   Once check accepted / work may be skipped
10  Once check miss / work required
11  trace exists but is not trusted
12  conflicting realization / nondeterminism detected
13  policy rejects IA dependency state
20  unsupported Nix version or feature
30  malformed configuration
40  internal/tooling error
```

Keep these in one module and document them.

---

# 6. CLI

The CLI vocabulary is part of the product. Prefer short commands that describe what Once is doing without implying stronger guarantees than the build trace provides.

Implement at least:

```text
once doctor
once check <installable>
once resolve <installable>
once trace <installable>
once trust <installable>
once explain <installable>
once run <installable>
```

Canonical user-facing language:

```text
The Once check passed.
The Once check missed.
The Once check is not trusted.
The Once check found a conflicting realization.
```

Avoid phrases such as "proof passed", "truth verified", or "computation proven".

## `doctor`

Print:

- detected Nix version;
- required minimum;
- whether `nix-command`, `flakes`, and `ca-derivations` work;
- configured substituters/trace backend;
- configured trusted key names;
- platform/system;
- whether the environment appears suitable for the POC.

Exit non-zero if required capabilities are absent.

## `explain`

Read-only full diagnostic command.

Example desired output:

```text
Closure Labs — Once

Installable:
  .#checks.x86_64-linux.demo-once

Nix:
  version: 2.35.2
  ca-derivations: enabled

Evaluation:
  unresolved derivation: /nix/store/...-demo-once.drv

Resolution:
  status: resolved
  base trace entries consulted: 3

Inputs:
  content-addressed: 2
  input-addressed: 18
  unknown: 0

Resolved check:
  /nix/store/...-demo-once.drv^out

Build trace:
  status: found
  output: /nix/store/...-demo-once
  trusted signatures: 1
  signer: ci.closurelabs.dev-1

Decision:
  ACCEPTED_WITH_IA_TRUST
  action: SKIP
```

If exact counts cannot be obtained from public Nix interfaces in v0.1, report `unknown` explicitly. Never invent counts.

## `check`

Perform the resolution/trust evaluation and return stable decision exit codes.

It must not execute the expensive derivation.

It should not download the expensive output merely to prove the trace exists.

This is one of the central POC acceptance tests.

## `resolve`

Resolve the current installable as far as Nix can using the configured base build trace. Show the unresolved derivation, resolved derivation/output identity, and any unknown state. This command is diagnostic and must not build.

## `trace`

Inspect the native Nix `build-trace-v2` state relevant to the installable. Show the resolved `.drv^output` key, realized output path, and available signature metadata without independently reimplementing signature verification.

## `trust`

Evaluate whether the trace/resolution chain satisfies the configured trust policy. This command is diagnostic and must fail closed on ambiguity.

## `run`

Algorithm:

```text
check target

if accepted:
    report SKIP
    exit 0

if MISS:
    build target using Nix
    check again
    require accepted trace after successful build
    report BUILT
    exit 0

if UNTRUSTED / CONFLICT / UNSUPPORTED:
    fail closed according to policy
```

`run` is orchestration only; Nix remains responsible for the actual build.

---

# 7. Configuration

Use a simple TOML config at:

```text
.once.toml
```

Example:

```toml
schema = 1

[once]
policy_version = "poc-v1"

[nix]
minimum_version = "2.35.0"
required_experimental_features = [
  "nix-command",
  "flakes",
  "ca-derivations",
]

[trust]
required_signatures = 1
accepted_key_names = [
  "ci.closurelabs.dev-1",
]
ia_mode = "warn"


[reporting]
github_summary = true
```

Supported IA modes for the POC:

```text
deny
warn
allow-trusted
```

Semantics:

- `deny`: any check-relevant IA dependency prevents a skip.
- `warn`: allow the Once check but classify it as `ACCEPTED_WITH_IA_TRUST`.
- `allow-trusted`: only allow IA dependencies when Nix can establish the configured trusted-cache/store-object policy.

If `allow-trusted` cannot be implemented safely through public Nix interfaces in v0.1, leave it explicitly unsupported rather than faking it.

---

# 8. Nix library and demo

Create a small Nix library function:

```text
lib.mkOnceCheck
```

Conceptual interface:

```nix
mkOnceCheck = {
  pkgs,
  name,
  target,
  policyVersion,
  check ? "",
  metadata ? {},
}: ...
```

The generated Once check derivation must:

- be content-addressed;
- depend on `target`;
- run any supplied verification step;
- include `policyVersion` in derivation semantics;
- emit a tiny deterministic payload;
- not embed the large target artifact into the check output.

Conceptual example only; adjust to valid Nix 2.35 syntax:

```nix
pkgs.runCommand "${name}-once-check" {
  __contentAddressed = true;
  outputHashAlgo = "sha256";
  outputHashMode = "nar";

  inherit policyVersion target;
} ''
  test -e "${target}"
  ${check}

  cat > "$out" <<EOF
  {"schema":"dev.closurelabs.once/v1","status":"ok","policy":"${policyVersion}"}
  EOF
''
```

The important invariant is:

```text
target changes
    ->
resolved Once check derivation changes
    ->
old base build-trace key does not satisfy the current Once check
```

even though the check output itself may remain byte-for-byte identical.

That is a feature, not a bug.

---

# 9. Demo derivation

Create a deterministic CA derivation such as:

```text
checks.x86_64-linux.demo-expensive
```

Requirements:

- deterministic;
- content-addressed;
- takes around 5-10 seconds in CI so the skip is visible without wasting runner time;
- depends on a tracked source file such as `demo/input.txt`;
- produces a moderately sized deterministic output;
- no network access;
- no timestamps/randomness in the output.

Example implementation strategy:

- sleep for 8 seconds;
- generate a deterministic file by repeating/hash-expanding the tracked input;
- write a manifest.

Then define:

```text
checks.x86_64-linux.demo-once
```

with `mkOnceCheck`, depending on `demo-expensive`.

The check output should remain tiny, ideally under 1 KiB.

---

# 10. The critical demonstration

Provide a reproducible script:

```text
scripts/demo.sh
```

It must demonstrate, with assertions:

## Phase A — cold build

1. `once check` -> MISS;
2. `once run`;
3. expensive derivation executes;
4. Once check derivation executes;
5. native build traces exist;
6. `once check` -> accepted.

## Phase B — delete expensive artifact

Delete or garbage-collect the expensive output while preserving the build-trace state.

Then:

1. check target again;
2. check must return accepted;
3. the expensive derivation must not execute;
4. the expensive artifact must not be re-downloaded merely for verification.

This is the most important proof-of-concept property.

## Phase C — relevant source mutation

Use a temporary Git worktree or generated fixture so the script does not dirty the user's checkout.

Change the source consumed by `demo-expensive`.

Expected result:

```text
old resolved derivation != new resolved derivation
`once check` -> MISS
```

## Phase D — irrelevant mutation

Change a README/docs-only input that is intentionally not part of the derivation inputs.

Expected:

```text
resolved Once check identity unchanged
`once check` -> accepted
```

## Phase E — policy invalidation

Change the `policyVersion` fed into the Once check.

Expected:

```text
resolved Once check identity changes
`once check` -> MISS
```

---

# 11. Trace conflict / nondeterminism behavior

The POC must contain a test for the following logical state:

```text
same resolved derivation output key
    ->
realization A

same resolved derivation output key
    ->
realization B
```

Do not silently pick one.

Expected decision:

```text
CONFLICT
```

Nix's local store already rejects incompatible registrations for one realization key. Preserve that fail-closed property at the application level.

This test may be implemented with isolated fixture data if producing a real nondeterministic CA conflict safely through the normal store is impractical.

Clearly label a fixture-based test as such.

---

# 12. Backend abstraction

Do not hard-code Cachix.

Create a minimal backend abstraction even if v0.1 only has one real implementation.

Conceptually:

```rust
trait TraceBackend {
    fn describe(&self) -> BackendDescription;
    fn configure_nix(&self, cmd: &mut Command) -> Result<()>;
}
```

Initial backends:

1. `LocalStore`
2. `FileBinaryCache` or another native Nix binary-cache target usable in local integration tests

Future adapters:

- Cachix;
- S3;
- Harmonia;
- generic HTTP binary cache.

The backend interface should mostly configure Nix. It should not become a second build-trace implementation.

For GitHub Actions, it is acceptable for v0.1 to demonstrate correctness using an isolated local/file-backed trace cache plus uploaded test artifacts. Do not contort the implementation into using GitHub Actions cache as if it were a Nix build-trace server.

Add an optional integration workflow for S3/Harmonia later if environment secrets are present.

---

# 13. Cryptographic trust requirements

Do not accept this as sufficient:

```text
trace JSON contains keyName == "ci.closurelabs.dev-1"
```

A signer label is not proof.

The POC must delegate actual signature validation/trusted registration to Nix/libstore.

Preferred sequence:

1. configure the trusted public key(s) through Nix;
2. ask Nix to consume/query/register the trace;
3. rely on Nix's trust checks;
4. only then treat the trace as trusted.

If upstream CLI behavior makes this impossible without downloading the large output, investigate the Nix C API/libstore narrowly.

Document any unavoidable limitation.

Never embed a private signing key in:

- the repository;
- the flake;
- a PR-visible environment;
- test fixtures that resemble production credentials.

For test signing, generate ephemeral keys during the integration test.

An unsigned local build-trace entry does not satisfy a policy requiring an
accepted signing key. Local-store trust and remote signed-cache trust must be
reported distinctly, and the canonical CI demonstration must exercise the
signed consumer path.

---

# 14. GitHub Actions

Create:

```text
.github/workflows/ci.yml
```

Triggers:

```yaml
pull_request:
push:
  branches: [main]
```

Requirements:

1. install upstream Nix >= 2.35;
2. assert the version;
3. enable required experimental features;
4. run formatting/lint;
5. run Rust tests;
6. run `nix flake check`;
7. run the end-to-end demo;
8. write a GitHub job summary explaining:
   - cold miss;
   - initial build;
   - trace hit;
   - artifact deletion;
   - trace-only reuse;
   - source invalidation;
   - policy invalidation.

For the POC, prioritize a deterministic self-contained workflow over persistent cross-PR infrastructure.

Also add:

```text
.github/CODEOWNERS
```

Protect, by convention:

```text
/.github/workflows/
/nix/
/.once.toml
```

Example owner placeholder:

```text
* @Closure-Labs/maintainers
```

If the actual GitHub organization/team slug is unknown, use a clearly documented placeholder rather than guessing.

Documentation must explain that CODEOWNERS only becomes an enforcement boundary when branch protection/rulesets require the review.

---

# 15. Rust implementation

Use stable Rust.

Suggested dependencies:

```text
clap
serde
serde_json
toml
thiserror
semver
tracing
tracing-subscriber
tempfile
```

Avoid a large dependency graph.

Suggested modules:

```text
src/
  main.rs
  cli.rs
  config.rs
  nix.rs
  policy.rs
  trace.rs
  backend.rs
  report.rs
  error.rs
```

Guidelines:

- no unsafe Rust unless there is a compelling C-API requirement;
- command execution must use argument arrays, never shell-concatenated strings;
- preserve stderr from Nix for diagnostics;
- provide structured internal types for decisions;
- normalize Nix JSON at one boundary module;
- handle slight Nix 2.35.x JSON differences defensively;
- do not parse human-readable terminal output when a JSON interface exists.

---

# 16. Repository layout

Create approximately:

```text
.
├── .once.toml
├── .github/
│   ├── CODEOWNERS
│   └── workflows/
│       └── ci.yml
├── demo/
│   └── input.txt
├── docs/
│   ├── architecture.md
│   ├── threat-model.md
│   ├── nix-2.35-notes.md
│   └── poc-demo.md
├── nix/
│   ├── once.nix
│   └── demo.nix
├── scripts/
│   └── demo.sh
├── src/
│   ├── backend.rs
│   ├── cli.rs
│   ├── config.rs
│   ├── error.rs
│   ├── main.rs
│   ├── nix.rs
│   ├── policy.rs
│   ├── report.rs
│   └── trace.rs
├── tests/
│   ├── cli.rs
│   ├── policy.rs
│   └── fixtures/
│       └── README.md
├── Cargo.lock
├── Cargo.toml
├── flake.lock
├── flake.nix
├── LICENSE
├── README.md
└── SECURITY.md
```

Do not add files merely to match this layout if they have no purpose.

---

# 17. Flake outputs

Expose at least:

```text
packages.x86_64-linux.default
packages.x86_64-linux.once

checks.x86_64-linux.rust-tests
checks.x86_64-linux.demo-expensive
checks.x86_64-linux.demo-once

devShells.x86_64-linux.default

formatter.x86_64-linux
```

Keep the flake understandable and avoid framework-heavy abstractions for v0.1.

A direct flake is preferable to adopting flake-parts unless it materially reduces complexity.

---

# 18. Documentation requirements

## README

The README should explain the project in plain technical language.

Suggested opening:

> **Once** is an experimental Closure Labs project for recognizing when an accepted realization of the same fully resolved Nix derivation has already occurred.
>
> **Build once. Recognize it thereafter.**
>
> Once uses Nix 2.35+ native build traces, content-addressed derivations, and configured trust policy. It does not create a new proof-of-computation protocol, and a passing Once check is a policy decision to accept a signed build-trace claim—not an independently auditable proof that arbitrary computation occurred.

Include:

- problem statement;
- architecture diagram;
- 60-second demo;
- security caveat;
- Nix version requirement;
- project status;
- roadmap.

## Threat model

`docs/threat-model.md` must cover:

- trusted versus untrusted builders;
- signing-key compromise;
- malicious PR changing the Once check definition;
- why trusted check policy should eventually be external to candidate-controlled code;
- input-addressed dependencies;
- nondeterminism;
- impure derivations;
- sandbox differences;
- Nix version differences;
- trace replay;
- trace conflicts;
- binary-cache compromise;
- why a valid trace is a trusted claim, not an independently auditable proof.

## Nix 2.35 notes

Document the assumptions the implementation depends on:

- build trace keyed by resolved derivation store path + output name;
- base build trace only;
- `build-trace-v2`;
- structured signatures;
- `nix store build-trace`;
- CA derivations remain experimental;
- input-addressed derivation resolution caveat.

---

# 19. Tests

Minimum unit tests:

- version parser:
  - 2.34.x rejected;
  - 2.35.0 accepted;
  - later versions accepted;
- config parser;
- exit-code mapping;
- policy classification;
- signer/trust result classification;
- conflict classification;
- IA-mode behavior.

Minimum integration tests:

1. cold Once check -> MISS;
2. build -> accepted;
3. repeat -> accepted without build;
4. relevant input change -> MISS;
5. policy version change -> MISS;
6. malformed trace -> fail closed;
7. untrusted trace -> UNTRUSTED;
8. incompatible trace -> CONFLICT;
9. Nix <2.35 mocked response -> UNSUPPORTED.

End-to-end Nix tests should run only when Nix >=2.35 and required features are available.

---

# 20. POC acceptance criteria

The POC is complete when all of these are true:

- `nix develop` provides a working development environment.
- `cargo test` passes.
- `nix flake check` passes.
- `once doctor` succeeds on Nix >=2.35 with CA enabled.
- `once check .#checks.x86_64-linux.demo-once` emits a compact result including resolved derivation, output name, trace status, signer/trust result, and `SKIP` or a rebuild-required decision.
- The demo expensive CA derivation visibly executes on a cold run.
- The tiny check derivation produces a small deterministic output.
- Native Nix build-trace state is visible after the build.
- A second Once check accepts the existing trace without rerunning the expensive derivation.
- After deleting the large output, Once check still succeeds from retained trace state without reconstructing or downloading that large output.
- A relevant source change invalidates the Once check.
- An irrelevant documentation change does not invalidate the Once check.
- A check-policy version change invalidates the Once check.
- Untrusted/conflicting/unknown states fail closed.
- GitHub Actions runs the complete demonstration.
- README and threat model use the Once terminology and do not call the mechanism trustless proof-of-computation.

---

# 21. Implementation order for Codex

Implement in this order.

## Milestone 1 — bootstrap

- initialize Rust CLI;
- add flake/dev shell;
- add Nix version check;
- add `doctor`;
- CI runs formatter/tests/flake check.

## Milestone 2 — native CA demo

- create deterministic slow CA derivation;
- create tiny CA Once check derivation;
- prove the Nix build creates build-trace state;
- document exact Nix 2.35 commands observed.

## Milestone 3 — trace inspection

- implement `explain`;
- normalize build-trace query results;
- show resolved derivation/output information where upstream interfaces expose it;
- do not overstate unavailable information.

## Milestone 4 — Once policy

- implement `check`;
- stable result enum and exit codes;
- fail closed;
- implement minimum trust policy through Nix.

## Milestone 5 — orchestration

- implement `run`;
- MISS -> build -> check;
- accepted -> skip.

## Milestone 6 — deletion/reuse test

- remove expensive realized output;
- retain trace;
- prove `once check` still succeeds without recreating/downloading the expensive output.

If this is not possible with the assumed upstream interfaces, stop and document exactly which Nix behavior prevents it. Do not fake the demo.

## Milestone 7 — invalidation

- relevant source mutation;
- irrelevant mutation;
- policy-version mutation.

## Milestone 8 — hardening/documentation

- conflict behavior;
- IA classification;
- threat model;
- GitHub summary;
- README polish.

---

# 22. Rules for implementation decisions

When ambiguity exists:

1. prefer upstream Nix behavior over custom behavior;
2. prefer a smaller POC over speculative abstractions;
3. fail closed on trust ambiguity;
4. report `unknown` rather than infer security properties;
5. do not add a cryptographic primitive that Nix already provides;
6. do not require a hosted service to run the core integration test;
7. keep vendor-specific backends outside the core;
8. preserve compatibility with Nix 2.35 semantics even if later Nix versions add conveniences;
9. document observed behavior if upstream documentation and actual 2.35 CLI output differ;
10. keep all security claims narrower than or equal to what can actually be demonstrated.

---

# 23. Future roadmap beyond v0.1

Add to README as future work:

- a separate protected Closure Labs "Once policy" flake/repository so candidate PR code cannot redefine what constitutes a passing Once check (implemented for v0.2);
- reusable GitHub Action (implemented for v0.2);
- Cachix adapter when native `build-trace-v2` behavior is verified;
- S3 backend using GitHub OIDC;
- Harmonia integration;
- stricter IA store-object lock/verification policy;
- multi-signature thresholds;
- Nix version/schema migration policy;
- SLSA/in-toto provenance as a separate audit layer;
- Sigstore identity binding;
- `aarch64-linux` and Darwin support;
- metrics showing build minutes saved;
- evaluation/resolution cache that is explicitly non-authoritative.

---

# 24. Canonical references Codex should consult

Use current upstream documentation and source, not memory, while implementing.

- Nix 2.35 release notes:
  `https://nix.dev/manual/nix/2.35/release-notes/rl-2.35.html`

- Nix 2.35 building model:
  `https://nix.dev/manual/nix/2.35/store/building.html`

- Nix 2.35 derivation resolution:
  `https://nix.dev/manual/nix/2.35/store/resolution.html`

- Nix 2.35 resolved derivation JSON:
  `https://nix.dev/manual/nix/2.35/protocols/json/derivation/resolved`

- Nix 2.35 CA derivation outputs:
  `https://nix.dev/manual/nix/2.35/store/derivation/outputs/content-address.html`

- Nix 2.35 experimental features:
  `https://nix.dev/manual/nix/2.35/development/experimental-features.html`

- `nix store build-trace` / `info` command reference:
  `https://nix.dev/manual/nix/2.35/command-ref/new-cli/nix3-store`
  `https://nix.dev/manual/nix/2.35/command-ref/new-cli/nix3-store-build-trace-info.html`

If behavior differs from documentation, inspect the Nix 2.35.x source and add a short note to `docs/nix-2.35-notes.md`.

---

# 25. Deliverable from Codex

Do not stop at an architecture document.

Produce a working repository.

Before considering the task complete:

```text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
nix flake check
./scripts/demo.sh
```

must succeed in the supported Nix 2.35+ Linux environment.

At the end, summarize:

- what was implemented;
- exact Nix behavior observed;
- which acceptance criteria passed;
- any Nix limitations encountered;
- any security properties that remain policy assumptions rather than technical guarantees;
- next three recommended engineering steps.

Do not publish releases, create external cloud resources, or add real credentials.

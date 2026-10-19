# GitHub Action

Once can reuse a signed check result on a fresh runner without downloading its
outputs. A protected producer builds and signs the native Nix trace; consumers
retrieve a small GitHub artifact and verify its signatures against an immutable
policy. GitHub artifacts provide transport, while the policy's public keys
authorize reuse.

Copy the [producer](../examples/github-actions-producer.yml) and
[consumer](../examples/github-actions-consumer.yml) examples into your repository.
Replace every Once action reference with the same full 40-character commit and
every policy reference with your immutable policy commit. Configure your check
with `mkOnceCheck`, embedding the policy's `policy_version`. Configure the public
signing key in the schema-1 policy’s `trust.public_keys` array (full Nix
`name:base64` key strings) and put its corresponding secret key only in
the protected producer's `ONCE_SIGNING_KEY` repository/environment secret. Protect
the producer branch before enabling these workflows.

A portable policy keeps the existing schema and supplies full keys, for example:

```toml
schema = 1
[once]
policy_version = "checks-v1"
[nix]
minimum_version = "2.35.3"
required_experimental_features = ["nix-command", "flakes", "ca-derivations"]
[trust]
required_signatures = 1
accepted_key_names = ["your-ci-1"]
public_keys = ["your-ci-1:REPLACE_WITH_BASE64_ED25519_PUBLIC_KEY"]
ia_mode = "warn"
```

Generate a Nix key pair with `nix key generate-secret --key-name your-ci-1` and
`nix key convert-secret-to-public`. Keep the private key out of the checkout.
Use the same `checks-v1` as `policyVersion` in `mkOnceCheck`. The policy flake
commit protects configuration; changing that commit alone does not force a
rebuild. Full keys also enable cryptographic verification in local mode.

The producer first checks existing evidence without signing credentials, so accepted
checks skip both rebuilding and publication until their artifact expires; only a
genuine `MISS` proceeds. It builds through root's local store with `secret-key-files`,
then deletes the temporary signing key. It publishes only after the build
succeeds on a push to the configured protected branch. The publish Action checks
the event, ref and protection again, exports signed trace evidence, and uploads
it with 30-day retention. An unchanged derivation and output share an artifact
name across source commits; changing the embedded policy version changes identity.
Only artifacts from a completed successful producer workflow run are discoverable.

The consumer uses `command: check`, no signing credentials, and read-only
`contents`/`actions` permissions. A genuine `MISS` runs the normal build. Invalid
signatures, conflicts, unsupported evidence and transport errors fail the job.
The example uses `pull_request` rather than privileged execution of candidate
code. Keep the workflow, action pin, policy pin and accepted keys protected; a
candidate-controlled workflow can always choose to omit enforcement. Never run
untrusted checkout contents through `pull_request_target` with privileged tokens.

## Inputs and outputs

`policy-flake` and `policy-revision` are required and retain the immutable policy
contract. `installable` is required for `check` and `run`, and omitted for `doctor`.
`command` defaults to `check`; the existing `doctor` and `run` modes remain available.

| Input | Default | Behavior |
| --- | --- | --- |
| `install-nix` | `true` | Optional installation of pinned Determinate Nix 3.23.3. Existing installations must report upstream 2.35.3. |
| `trace-bundle` | empty | Local bundle directory; valid only with `check`. |
| `producer-workflow` | empty | Filename or numeric ID of the producer workflow in this repository; enables artifact lookup for `check`. |
| `producer-branch` | `main` | Existing protected branch whose successful push runs may provide bundles. |
| `github-token` | `${{ github.token }}` | Token with `actions: read` and `contents: read`; needed only for artifact lookup. |
| `fail-on-nonzero` | `true` | Fail after recording outputs for nonzero Once results. Set false to branch explicitly on `MISS`. |

`trace-bundle` and `producer-workflow` are mutually exclusive. Without either,
the existing local check behavior applies. The wrapper builds Once from the
Action's own source and lock file with the explicit signed `once.cachix.org`
substituter and public key. A cache miss falls back to building Once from source;
it does not substitute or build the candidate check.

| Output | Meaning |
| --- | --- |
| `result-json` | Versioned CLI JSON, including a normal CLI `MISS` when transport found no artifact. |
| `schema` | `dev.closurelabs.once/doctor/v1` or `dev.closurelabs.once/result/v1`. |
| `exit-code` | Once's numeric exit code; `10` means a genuine miss. |
| `cache-hit` | `true` only for an accepted decision with exit code zero. |
| `decision` | Once decision, empty for `doctor` or when no decision was emitted. |
| `bundle-id` | Stable portable artifact name when bundle lookup or a local bundle is requested. |

Missing workflows, missing artifacts, expired artifacts and downloads returning
404/410 are ordinary misses. Authentication, network, protection-check and archive
errors fail the Action even when `fail-on-nonzero` is false. Candidate jobs need
no upload permission. Fork pull requests use the base repository's read-only token;
repository settings must allow reading that repository's Actions artifacts.

Portable mode currently supports static content-addressed check derivations on
`x86_64-linux`, with the policy's disclosed IA dependency trust. Successful reuse
does not restore output paths. Artifact retention or GitHub service availability
can reduce reuse but cannot authorize an unsigned result.

The optional installer is the release-pinned
[Determinate Nix Action](https://github.com/DeterminateSystems/determinate-nix-action),
whose version tags pin the Nix release. Pinning the older `nix-installer-action`
alone did not pin the downloaded Nix version. When using an existing installation, install the tested version explicitly and set
`install-nix: "false"`.


A check's identity follows its actual Nix inputs. Files included in its source
can invalidate reuse even when they are documentation; apply appropriate Nix
source filters when those files are irrelevant to the check.

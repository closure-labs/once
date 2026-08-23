#!/usr/bin/env bash
set -euo pipefail
umask 077

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
case_runner="$repo_root/scripts/build-trace-version-case.sh"

baseline_revision=2c423e03bbafcff28bfadc6781a4a8257f205cb5
baseline_installable="github:NixOS/nixpkgs/$baseline_revision#nixVersions.latest"
forward_revision=88b09c64fbea076a0376830d98e5331f70ed31a3
forward_installable="github:NixOS/nix/$forward_revision"

for command in nix jq; do
  if ! command -v "$command" > /dev/null; then
    echo "$command is required" >&2
    exit 1
  fi
done
if [[ ! -x "$case_runner" ]]; then
  echo "Case runner is not executable: $case_runner" >&2
  exit 1
fi

matrix_tmp_parent=${RUNNER_TEMP:-${TMPDIR:-/tmp}}
matrix_tmp=$(mktemp -d -p "$matrix_tmp_parent" once-build-trace-version-matrix.XXXXXXXX)
cleanup() {
  if [[ ${ONCE_MATRIX_KEEP_RESULTS:-0} == 1 ]]; then
    echo "Kept matrix results at $matrix_tmp" >&2
    return
  fi
  chmod -R u+w "$matrix_tmp" 2>/dev/null || true
  rm -rf -- "$matrix_tmp"
}
trap cleanup EXIT

base_nix_config='extra-experimental-features = nix-command flakes ca-derivations'

run_case() {
  local label=$1
  local installable=$2
  local result="$matrix_tmp/$label.json"
  local log="$matrix_tmp/$label.log"

  echo "Running $label build-trace case" >&2
  if ! env NIX_CONFIG="$base_nix_config" \
    nix shell "$installable" -c "$case_runner" > "$result" 2> "$log"; then
    echo "$label case failed; last 80 log lines follow" >&2
    tail -80 "$log" >&2
    return 1
  fi
}

run_case nix-2.35.2 "$baseline_installable"
run_case nix-2.36-pre "$forward_installable"

baseline_result="$matrix_tmp/nix-2.35.2.json"
forward_result="$matrix_tmp/nix-2.36-pre.json"

jq -e '
  .nixVersion == "nix (Nix) 2.35.2" and
  .readOnlyRemoteQuery == {
    "correctKeyReturnedTrace": true,
    "referencedOutputNarRequested": false,
    "unrelatedKeyReturnedIdenticalTrace": true
  } and
  .onceRemoteDecision == {"decision": "UNSUPPORTED", "exitCode": 20} and
  .substitution == {
    "buildTraceSignatureRejected": false,
    "correctKeyRealizedOutput": true,
    "unrelatedKeyExitCode": 0,
    "unrelatedKeyRealizedOutput": true
  }
' "$baseline_result" > /dev/null

jq -e '
  .nixVersion == "nix (Nix) 2.36.0pre20260822_88b09c6" and
  .readOnlyRemoteQuery == {
    "correctKeyReturnedTrace": true,
    "referencedOutputNarRequested": false,
    "unrelatedKeyReturnedIdenticalTrace": true
  } and
  .onceRemoteDecision == {"decision": "UNSUPPORTED", "exitCode": 20} and
  .substitution.buildTraceSignatureRejected == true and
  .substitution.correctKeyRealizedOutput == true and
  .substitution.unrelatedKeyExitCode != 0 and
  .substitution.unrelatedKeyRealizedOutput == false
' "$forward_result" > /dev/null

jq -e --slurpfile forward "$forward_result" '
  .unresolvedDerivation == $forward[0].unresolvedDerivation and
  .resolvedDerivation == $forward[0].resolvedDerivation and
  .outputPath == $forward[0].outputPath
' "$baseline_result" > /dev/null

jq -n \
  --arg schema 'dev.closurelabs.once/build-trace-version-matrix/v1' \
  --arg baselineRevision "$baseline_revision" \
  --arg baselineInstallable "$baseline_installable" \
  --arg forwardRevision "$forward_revision" \
  --arg forwardInstallable "$forward_installable" \
  --slurpfile baseline "$baseline_result" \
  --slurpfile forward "$forward_result" \
  '{
    schema: $schema,
    pins: {
      baseline: {
        revision: $baselineRevision,
        installable: $baselineInstallable
      },
      forward: {
        revision: $forwardRevision,
        installable: $forwardInstallable
      }
    },
    results: {
      baseline: $baseline[0],
      forward: $forward[0]
    }
  }'

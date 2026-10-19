#!/usr/bin/env bash
set -euo pipefail
repository_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/bin" "$fixture/store/bin"
cat >"$fixture/bin/nix" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$FAKE_NIX_LOG"
if [[ "$*" == --version ]]; then
  echo "nix (Determinate Nix 3.23.3) ${FAKE_NIX_VERSION:-2.35.3}"
elif [[ "$*" == *"build path:"*"#once --no-link --print-out-paths --no-write-lock-file" ]]; then
  echo "$FAKE_ONCE_STORE"
else
  echo "unexpected fake Nix arguments: $*" >&2
  exit 2
fi
MOCK
cat >"$fixture/store/bin/once" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$FAKE_ONCE_LOG"
for argument in "$@"; do
  case "$argument" in
    doctor) echo '{"schema":"dev.closurelabs.once/doctor/v1","suitable":true}'; exit "${FAKE_ONCE_EXIT:-0}" ;;
    bundle-key) echo once-v1-aabbccdd; exit 0 ;;
    check | run) printf '{"schema":"dev.closurelabs.once/result/v1","decision":"%s"}\n' "${FAKE_ONCE_DECISION:-MISS}"; exit "${FAKE_ONCE_EXIT:-10}" ;;
    export) [[ ! -e "${!#}" ]] || { echo "export destination already exists" >&2; exit 40; }; mkdir -p "${!#}"; exit 0 ;;
  esac
done
exit 2
MOCK
real_python=$(command -v python3)
cat >"$fixture/bin/python3" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == */scripts/github-artifacts.py ]]; then
  if [[ "${FAKE_TRANSPORT_STATUS:-0}" == 0 ]]; then
    while [[ $# -gt 0 ]]; do
      if [[ "$1" == --output ]]; then mkdir -p "$2"; break; fi
      shift
    done
  fi
  exit "${FAKE_TRANSPORT_STATUS:-0}"
fi
exec "$REAL_PYTHON" "$@"
MOCK
export REAL_PYTHON="$real_python"
chmod +x "$fixture/bin/python3"
chmod +x "$fixture/bin/nix" "$fixture/store/bin/once"
export PATH="$fixture/bin:$PATH" FAKE_ONCE_STORE="$fixture/store"
export FAKE_NIX_LOG="$fixture/nix.log" FAKE_ONCE_LOG="$fixture/once.log"
export GITHUB_ACTION_PATH="$repository_root" GITHUB_OUTPUT="$fixture/output" GITHUB_STEP_SUMMARY="$fixture/summary"
export ONCE_ACTION_POLICY_FLAKE=github:closure-labs/once-policy/0123456789abcdef0123456789abcdef01234567
export ONCE_ACTION_POLICY_REVISION=0123456789abcdef0123456789abcdef01234567
run_action() {
  : >"$fixture/output"
  : >"$fixture/summary"
  ONCE_ACTION_COMMAND=$1 ONCE_ACTION_INSTALLABLE=$2 FAKE_ONCE_EXIT=$3 \
    bash "$repository_root/scripts/github-action.sh"
}
run_action doctor "" 0
grep -q '^schema=dev.closurelabs.once/doctor/v1$' "$fixture/output"
grep -q '^cache-hit=false$' "$fixture/output"
grep -q 'Once GitHub Action' "$fixture/summary"
grep -q -- '--option extra-substituters https://once.cachix.org' "$fixture/nix.log"
run_action check .#integration 10
grep -q '^decision=MISS$' "$fixture/output"
grep -q '^exit-code=10$' "$fixture/output"
FAKE_ONCE_DECISION=ACCEPTED_CA run_action check .#integration 0
grep -q '^cache-hit=true$' "$fixture/output"
FAKE_ONCE_DECISION=ACCEPTED_WITH_IA_TRUST run_action check .#integration 0
grep -q '^cache-hit=true$' "$fixture/output"
FAKE_ONCE_DECISION=ACCEPTED_CA run_action check .#integration 11 2>/dev/null
grep -q '^cache-hit=false$' "$fixture/output"
grep -q '^exit-code=40$' "$fixture/output"
FAKE_ONCE_DECISION=UNKNOWN run_action check .#integration 0 2>/dev/null
grep -q '^cache-hit=false$' "$fixture/output"
grep -q '^exit-code=40$' "$fixture/output"
FAKE_ONCE_DECISION=UNTRUSTED run_action check .#integration 11
grep -q '^cache-hit=false$' "$fixture/output"
ONCE_ACTION_TRACE_BUNDLE="$fixture/bundle with spaces" run_action check .#integration 10
grep -q '^bundle-id=once-v1-aabbccdd$' "$fixture/output"
grep -q -- '--trace-bundle' "$fixture/once.log"
if run_action trace .#integration 0 2>/dev/null; then echo 'invalid command accepted' >&2; exit 1; fi
if run_action check "" 0 2>/dev/null; then echo 'missing installable accepted' >&2; exit 1; fi
if ONCE_ACTION_FAIL_ON_NONZERO=maybe run_action doctor "" 0 2>/dev/null; then echo 'invalid boolean accepted' >&2; exit 1; fi
if FAKE_NIX_VERSION=2.36.0 run_action doctor "" 0 2>/dev/null; then echo 'untested Nix accepted' >&2; exit 1; fi
if ONCE_ACTION_TRACE_BUNDLE=x ONCE_ACTION_PRODUCER_WORKFLOW=ci.yml run_action check .#x 0 2>/dev/null; then echo 'ambiguous transport accepted' >&2; exit 1; fi
if ONCE_ACTION_TRACE_BUNDLE=x run_action run .#x 0 2>/dev/null; then echo 'bundle run accepted' >&2; exit 1; fi
ONCE_ACTION_PRODUCER_WORKFLOW=ci.yml FAKE_TRANSPORT_STATUS=10 run_action check .#integration 10
grep -q '^decision=MISS$' "$fixture/output"
grep -q '^exit-code=10$' "$fixture/output"
grep -q '^cache-hit=false$' "$fixture/output"
ONCE_ACTION_PRODUCER_WORKFLOW=ci.yml FAKE_TRANSPORT_STATUS=0 FAKE_ONCE_DECISION=ACCEPTED_CA run_action check .#integration 0
grep -q '^cache-hit=true$' "$fixture/output"
if ONCE_ACTION_PRODUCER_WORKFLOW=ci.yml FAKE_TRANSPORT_STATUS=40 ONCE_ACTION_FAIL_ON_NONZERO=false run_action check .#integration 0; then
  echo 'transport error did not fail' >&2; exit 1
fi
# Publishing must reject PRs, unprotected branches, and other branches before Nix runs.
for event in pull_request push; do
  if GITHUB_EVENT_NAME="$event" GITHUB_REF=refs/heads/main GITHUB_REF_PROTECTED=false \
      bash "$repository_root/scripts/github-publish.sh" 2>/dev/null; then
    echo 'unprotected publish accepted' >&2; exit 1
  fi
done
if GITHUB_EVENT_NAME=push GITHUB_REF=refs/heads/other GITHUB_REF_PROTECTED=true \
    bash "$repository_root/scripts/github-publish.sh" 2>/dev/null; then
  echo 'wrong publishing branch accepted' >&2; exit 1
fi
GITHUB_ACTION_PATH="$repository_root/publish" GITHUB_EVENT_NAME=push GITHUB_REF=refs/heads/main GITHUB_REF_PROTECTED=true \
    ONCE_ACTION_INSTALLABLE=.#integration RUNNER_TEMP="$fixture" bash "$repository_root/scripts/github-publish.sh"
grep -q '^bundle-id=once-v1-aabbccdd$' "$fixture/output"
grep -q 'export .#integration --output' "$fixture/once.log"
echo 'GitHub Action wrapper tests passed'

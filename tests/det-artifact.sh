#!/usr/bin/env bash
set -euo pipefail
umask 077

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
det_tool_path="$repo_root/scripts/det-artifact.sh"
fixture=$(mktemp -d)
cleanup() {
  chmod -R u+w "$fixture" 2> /dev/null || true
  rm -rf -- "$fixture"
}
trap cleanup EXIT

det_tool() {
  bash "$det_tool_path" "$@"
}

expect_failure() {
  if "$@" > "$fixture/unexpected.stdout" 2> "$fixture/unexpected.stderr"; then
    echo "command unexpectedly succeeded: $*" >&2
    exit 1
  fi
}

printf '%s\n' '{"z":true,"nested":{"b":2,"a":1},"schema":"example/v1"}' \
  > "$fixture/payload-a.json"
printf '%s\n' '{"schema":"example/v1","nested":{"a":1,"b":2},"z":true}' \
  > "$fixture/payload-b.json"

key_id=$(
  det_tool keygen \
    --private-key "$fixture/signing.det-private.pem" \
    --public-key "$fixture/signing.det-public.pem"
)
[[ "$key_id" == sha256:* ]]
[[ $(stat -c '%a' "$fixture/signing.det-private.pem") == 600 ]]

det_tool create \
  --payload "$fixture/payload-a.json" \
  --payload-type application/vnd.closurelabs.once.test.v1+json \
  --private-key "$fixture/signing.det-private.pem" \
  --output "$fixture/a.det"
det_tool create \
  --payload "$fixture/payload-b.json" \
  --payload-type application/vnd.closurelabs.once.test.v1+json \
  --private-key "$fixture/signing.det-private.pem" \
  --output "$fixture/b.det"

cmp -s "$fixture/a.det" "$fixture/b.det"
jq -e --arg keyId "$key_id" '
  .schema == "dev.closurelabs.det/v1" and
  .mediaType == "application/vnd.closurelabs.det+json" and
  .signatures[0].algorithm == "Ed25519" and
  .signatures[0].keyId == $keyId
' "$fixture/a.det" > /dev/null
if grep -Fq 'PRIVATE KEY' "$fixture/a.det"; then
  echo "artifact contains private key material" >&2
  exit 1
fi

det_tool verify \
  --artifact "$fixture/a.det" \
  --trusted-public-key "$fixture/signing.det-public.pem" \
  > "$fixture/verified.json"
jq -e '
  .schema == "example/v1" and
  .nested == {"a": 1, "b": 2} and
  .z == true
' "$fixture/verified.json" > /dev/null

det_tool verify --artifact "$fixture/a.det" \
  > /dev/null 2> "$fixture/unpinned.stderr"
grep -Fq 'signer identity is not authorized' "$fixture/unpinned.stderr"

det_tool keygen \
  --private-key "$fixture/unrelated.det-private.pem" \
  --public-key "$fixture/unrelated.det-public.pem" > /dev/null
expect_failure det_tool verify \
  --artifact "$fixture/a.det" \
  --trusted-public-key "$fixture/unrelated.det-public.pem"

tampered_payload=$(printf '%s' '{"tampered":true}' | openssl base64 -A)
jq -c --arg payload "$tampered_payload" '.payload = $payload' \
  "$fixture/a.det" > "$fixture/tampered.det"
expect_failure det_tool verify \
  --artifact "$fixture/tampered.det" \
  --trusted-public-key "$fixture/signing.det-public.pem"

jq -c '.unsignedField = true' "$fixture/a.det" > "$fixture/extra-field.det"
expect_failure det_tool verify --artifact "$fixture/extra-field.det"

jq -cj '.payload += "\n"' "$fixture/a.det" > "$fixture/noncanonical-base64.det"
expect_failure det_tool verify --artifact "$fixture/noncanonical-base64.det"

jq . "$fixture/a.det" > "$fixture/noncanonical-envelope.det"
expect_failure det_tool verify --artifact "$fixture/noncanonical-envelope.det"

ln -s "$fixture/a.det" "$fixture/artifact-symlink.det"
expect_failure det_tool verify --artifact "$fixture/artifact-symlink.det"

chmod 0644 "$fixture/signing.det-private.pem"
expect_failure det_tool create \
  --payload "$fixture/payload-a.json" \
  --payload-type application/vnd.closurelabs.once.test.v1+json \
  --private-key "$fixture/signing.det-private.pem" \
  --output "$fixture/insecure.det"
chmod 0600 "$fixture/signing.det-private.pem"

expect_failure det_tool create \
  --payload "$fixture/payload-a.json" \
  --payload-type application/vnd.closurelabs.once.test.v1+json \
  --private-key "$fixture/signing.det-private.pem" \
  --output "$fixture/not-a-det.json"

printf '%s\n' '{"unsafeNumber":1.5}' > "$fixture/float-payload.json"
expect_failure det_tool create \
  --payload "$fixture/float-payload.json" \
  --payload-type application/vnd.closurelabs.once.test.v1+json \
  --private-key "$fixture/signing.det-private.pem" \
  --output "$fixture/float.det"

openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 \
  -out "$fixture/unsupported.det-private.pem" 2> /dev/null
chmod 0600 "$fixture/unsupported.det-private.pem"
expect_failure det_tool create \
  --payload "$fixture/payload-a.json" \
  --payload-type application/vnd.closurelabs.once.test.v1+json \
  --private-key "$fixture/unsupported.det-private.pem" \
  --output "$fixture/unsupported-key.det"

echo ".det artifact tests passed"

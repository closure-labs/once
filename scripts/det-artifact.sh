#!/usr/bin/env bash
set -euo pipefail
umask 077

det_schema='dev.closurelabs.det/v1'
det_media_type='application/vnd.closurelabs.det+json'
max_artifact_bytes=$((10 * 1024 * 1024))
cleanup_paths=()

cleanup() {
  local path
  for path in "${cleanup_paths[@]}"; do
    if [[ -d "$path" && ! -L "$path" ]]; then
      chmod -R u+w "$path" 2> /dev/null || true
      rm -rf -- "$path"
    else
      rm -f -- "$path"
    fi
  done
}
trap cleanup EXIT

die() {
  echo "det: $*" >&2
  exit 1
}

usage() {
  cat >&2 <<'EOF'
Usage:
  det-artifact.sh keygen --private-key PATH --public-key PATH
  det-artifact.sh create --payload PATH --payload-type TYPE \
    --private-key PATH --output PATH.det
  det-artifact.sh verify --artifact PATH.det [--trusted-public-key PATH]
EOF
  exit 2
}

require_commands() {
  local command
  for command in jq openssl; do
    command -v "$command" > /dev/null || die "$command is required"
  done
}

require_parent_directory() {
  local path=$1
  local parent
  parent=$(dirname "$path")
  [[ -d "$parent" ]] || die "parent directory does not exist: $parent"
}

require_new_path() {
  local path=$1
  [[ ! -e "$path" && ! -L "$path" ]] || die "refusing to overwrite: $path"
  require_parent_directory "$path"
}

install_without_overwrite() {
  local source=$1
  local destination=$2
  if ! ln "$source" "$destination" 2> /dev/null; then
    die "could not create without overwriting: $destination"
  fi
}

canonicalize_object() {
  local input=$1
  local output=$2
  jq -cSj '
    def deterministic_value:
      if type == "number" then
        . == floor and . >= -9007199254740991 and . <= 9007199254740991
      elif type == "array" or type == "object" then
        all(.[]; deterministic_value)
      else
        true
      end;
    if type != "object" then
      error("payload must be a JSON object")
    elif deterministic_value | not then
      error("payload numbers must be safe integers")
    else
      .
    end,
    (inputs | error("payload must contain exactly one JSON value"))
  ' "$input" > "$output"
}

make_pae() {
  local payload_type=$1
  local payload=$2
  local output=$3
  local payload_size
  payload_size=$(wc -c < "$payload")
  {
    printf 'DSSEv1 %s %s %s ' "${#payload_type}" "$payload_type" "$payload_size"
    cat "$payload"
  } > "$output"
}

public_key_id() {
  local public_der=$1
  local digest
  digest=$(openssl dgst -sha256 -binary "$public_der" | openssl base64 -A)
  printf 'sha256:%s' "$digest"
}

require_ed25519_public_key() {
  local public_der=$1
  local description
  description=$(openssl pkey -pubin -inform DER -in "$public_der" -text_pub -noout 2> /dev/null) \
    || die "public key is not valid SPKI DER"
  [[ "$description" == ED25519\ Public-Key:* ]] || die "only Ed25519 keys are supported"
}

check_private_key_permissions() {
  local private_key=$1
  local mode
  local mode_value
  mode=$(stat -c '%a' "$private_key")
  mode_value=$((8#$mode))
  if ((mode_value & 077)); then
    die "private key must not be accessible by group or other users: $private_key"
  fi
}

keygen() {
  local private_key=''
  local public_key=''

  while (($#)); do
    case "$1" in
      --private-key)
        (($# >= 2)) || usage
        private_key=$2
        shift 2
        ;;
      --public-key)
        (($# >= 2)) || usage
        public_key=$2
        shift 2
        ;;
      *) usage ;;
    esac
  done

  [[ -n "$private_key" && -n "$public_key" ]] || usage
  require_new_path "$private_key"
  require_new_path "$public_key"

  local private_parent
  local public_parent
  local private_tmp
  local public_tmp
  local public_der
  private_parent=$(dirname "$private_key")
  public_parent=$(dirname "$public_key")
  private_tmp=$(mktemp -p "$private_parent" .det-private.XXXXXXXX)
  public_tmp=$(mktemp -p "$public_parent" .det-public.XXXXXXXX)
  public_der=$(mktemp -p "$public_parent" .det-public-der.XXXXXXXX)
  cleanup_paths+=("$private_tmp" "$public_tmp" "$public_der")

  openssl genpkey -algorithm ED25519 -out "$private_tmp"
  chmod 0600 "$private_tmp"
  openssl pkey -in "$private_tmp" -pubout -out "$public_tmp"
  chmod 0644 "$public_tmp"
  openssl pkey -pubin -in "$public_tmp" -outform DER -out "$public_der"

  install_without_overwrite "$private_tmp" "$private_key"
  install_without_overwrite "$public_tmp" "$public_key"
  public_key_id "$public_der"
  printf '\n'
}

create_artifact() {
  local payload=''
  local payload_type=''
  local private_key=''
  local output=''

  while (($#)); do
    case "$1" in
      --payload)
        (($# >= 2)) || usage
        payload=$2
        shift 2
        ;;
      --payload-type)
        (($# >= 2)) || usage
        payload_type=$2
        shift 2
        ;;
      --private-key)
        (($# >= 2)) || usage
        private_key=$2
        shift 2
        ;;
      --output)
        (($# >= 2)) || usage
        output=$2
        shift 2
        ;;
      *) usage ;;
    esac
  done

  [[ -n "$payload" && -n "$payload_type" && -n "$private_key" && -n "$output" ]] || usage
  [[ "$output" == *.det ]] || die "artifact output must use the .det extension"
  [[ "$payload_type" =~ ^[A-Za-z0-9][A-Za-z0-9.+/-]*$ ]] || die "payload type must be an ASCII media type"
  [[ -f "$payload" ]] || die "payload is not a regular file: $payload"
  [[ -f "$private_key" && ! -L "$private_key" ]] || die "private key is not a regular non-symlink file"
  require_new_path "$output"
  check_private_key_permissions "$private_key"

  local output_parent
  local work
  output_parent=$(dirname "$output")
  work=$(mktemp -d -p "$output_parent" .det-create.XXXXXXXX)
  cleanup_paths+=("$work")

  local canonical_payload="$work/payload.json"
  local public_der="$work/public.der"
  local pae="$work/pae"
  local signature="$work/signature"
  local envelope="$work/envelope.det"
  local private_key_snapshot="$work/private.pem"

  canonicalize_object "$payload" "$canonical_payload"
  local canonical_size
  canonical_size=$(wc -c < "$canonical_payload")
  ((canonical_size <= 7 * 1024 * 1024)) || die "canonical payload exceeds the 7 MiB creation limit"
  cp -- "$private_key" "$private_key_snapshot"
  chmod 0600 "$private_key_snapshot"
  openssl pkey -in "$private_key_snapshot" -pubout -outform DER -out "$public_der"
  require_ed25519_public_key "$public_der"
  make_pae "$payload_type" "$canonical_payload" "$pae"
  openssl pkeyutl -sign -rawin -inkey "$private_key_snapshot" -in "$pae" -out "$signature"
  [[ $(wc -c < "$signature") -eq 64 ]] || die "Ed25519 signature must be 64 bytes"

  local key_id
  local payload_base64
  local public_base64
  local signature_base64
  key_id=$(public_key_id "$public_der")
  payload_base64=$(openssl base64 -A -in "$canonical_payload")
  public_base64=$(openssl base64 -A -in "$public_der")
  signature_base64=$(openssl base64 -A -in "$signature")

  jq -cjnS \
    --arg schema "$det_schema" \
    --arg mediaType "$det_media_type" \
    --arg payloadType "$payload_type" \
    --arg payload "$payload_base64" \
    --arg keyId "$key_id" \
    --arg publicKey "$public_base64" \
    --arg signature "$signature_base64" \
    '{
      schema: $schema,
      mediaType: $mediaType,
      payloadType: $payloadType,
      payloadEncoding: "base64",
      payload: $payload,
      signatures: [{
        algorithm: "Ed25519",
        keyId: $keyId,
        publicKey: {
          format: "spki-der",
          encoding: "base64",
          value: $publicKey
        },
        value: $signature
      }]
    }' > "$envelope"

  install_without_overwrite "$envelope" "$output"
  echo "Created $output with signer $key_id" >&2
}

verify_artifact() {
  local artifact=''
  local trusted_public_key=''

  while (($#)); do
    case "$1" in
      --artifact)
        (($# >= 2)) || usage
        artifact=$2
        shift 2
        ;;
      --trusted-public-key)
        (($# >= 2)) || usage
        trusted_public_key=$2
        shift 2
        ;;
      *) usage ;;
    esac
  done

  [[ -n "$artifact" ]] || usage
  [[ -f "$artifact" && ! -L "$artifact" ]] || die "artifact is not a regular non-symlink file: $artifact"

  local work
  work=$(mktemp -d)
  cleanup_paths+=("$work")
  local artifact_snapshot="$work/artifact.det"
  cp -- "$artifact" "$artifact_snapshot"
  chmod 0600 "$artifact_snapshot"

  local artifact_size
  artifact_size=$(wc -c < "$artifact_snapshot")
  ((artifact_size <= max_artifact_bytes)) || die "artifact exceeds the 10 MiB verification limit"

  jq -e \
    --arg schema "$det_schema" \
    --arg mediaType "$det_media_type" '
      type == "object" and
      keys == ["mediaType", "payload", "payloadEncoding", "payloadType", "schema", "signatures"] and
      .schema == $schema and
      .mediaType == $mediaType and
      .payloadEncoding == "base64" and
      (.payloadType | type == "string" and test("^[A-Za-z0-9][A-Za-z0-9.+/-]*$")) and
      (.payload | type == "string") and
      (.signatures | type == "array" and length == 1) and
      (.signatures[0] | type == "object" and
        keys == ["algorithm", "keyId", "publicKey", "value"] and
        .algorithm == "Ed25519" and
        (.keyId | type == "string") and
        (.value | type == "string") and
        (.publicKey | type == "object" and
          keys == ["encoding", "format", "value"] and
          .format == "spki-der" and
          .encoding == "base64" and
          (.value | type == "string")))
    ' "$artifact_snapshot" > /dev/null || die "artifact envelope is not valid $det_schema JSON"

  local canonical_envelope="$work/artifact.canonical.det"
  canonicalize_object "$artifact_snapshot" "$canonical_envelope" || die "artifact envelope is not canonical JSON"
  cmp -s "$artifact_snapshot" "$canonical_envelope" || die "artifact envelope bytes are not canonical JSON"

  local payload_encoded="$work/payload.base64"
  local payload="$work/payload.json"
  local canonical_payload="$work/payload.canonical.json"
  local public_encoded="$work/public.base64"
  local public_der="$work/public.der"
  local signature_encoded="$work/signature.base64"
  local signature="$work/signature"
  local pae="$work/pae"

  jq -j '.payload' "$artifact_snapshot" > "$payload_encoded"
  jq -j '.signatures[0].publicKey.value' "$artifact_snapshot" > "$public_encoded"
  jq -j '.signatures[0].value' "$artifact_snapshot" > "$signature_encoded"
  openssl base64 -d -A -in "$payload_encoded" -out "$payload" || die "payload is not valid base64"
  openssl base64 -d -A -in "$public_encoded" -out "$public_der" || die "public key is not valid base64"
  openssl base64 -d -A -in "$signature_encoded" -out "$signature" || die "signature is not valid base64"

  local encoded_check="$work/encoded.check"
  openssl base64 -A -in "$payload" -out "$encoded_check"
  cmp -s "$payload_encoded" "$encoded_check" || die "payload is not canonical base64"
  openssl base64 -A -in "$public_der" -out "$encoded_check"
  cmp -s "$public_encoded" "$encoded_check" || die "public key is not canonical base64"
  openssl base64 -A -in "$signature" -out "$encoded_check"
  cmp -s "$signature_encoded" "$encoded_check" || die "signature is not canonical base64"
  [[ $(wc -c < "$signature") -eq 64 ]] || die "Ed25519 signature must be 64 bytes"

  canonicalize_object "$payload" "$canonical_payload" || die "payload is not valid canonical JSON"
  cmp -s "$payload" "$canonical_payload" || die "payload bytes are not canonical JSON"
  require_ed25519_public_key "$public_der"

  local expected_key_id
  local embedded_key_id
  expected_key_id=$(public_key_id "$public_der")
  embedded_key_id=$(jq -r '.signatures[0].keyId' "$artifact_snapshot")
  [[ "$embedded_key_id" == "$expected_key_id" ]] || die "embedded key ID does not match the public key"

  if [[ -n "$trusted_public_key" ]]; then
    [[ -f "$trusted_public_key" && ! -L "$trusted_public_key" ]] || die "trusted public key is not a regular non-symlink file"
    local trusted_der="$work/trusted.der"
    openssl pkey -pubin -in "$trusted_public_key" -outform DER -out "$trusted_der" > /dev/null 2>&1 || die "trusted public key is not valid PEM"
    cmp -s "$trusted_der" "$public_der" || die "artifact signer does not match the trusted public key"
  fi

  local payload_type
  payload_type=$(jq -r '.payloadType' "$artifact_snapshot")
  make_pae "$payload_type" "$payload" "$pae"
  openssl pkeyutl -verify -pubin -keyform DER -inkey "$public_der" -rawin \
    -in "$pae" -sigfile "$signature" > /dev/null 2>&1 || die "artifact signature verification failed"

  if [[ -z "$trusted_public_key" ]]; then
    echo "det: signature is internally valid but signer identity is not authorized without --trusted-public-key" >&2
  fi

  echo "Verified $artifact with signer $expected_key_id" >&2
  cat "$payload"
  printf '\n'
}

require_commands
command_name=${1:-}
[[ -n "$command_name" ]] || usage
shift

case "$command_name" in
  keygen) keygen "$@" ;;
  create) create_artifact "$@" ;;
  verify) verify_artifact "$@" ;;
  *) usage ;;
esac

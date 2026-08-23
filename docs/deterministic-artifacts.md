# Deterministic `.det` artifacts

Once's research harness can package its substitution evidence as a `.det`
file: a deterministic, signed JSON envelope. The proof of concept selects
`.det` rather than the shorter `.da` extension so the file's purpose remains
recognizable.

## What it establishes

A valid `.det` signature establishes that its canonical payload has not
changed since the holder of the corresponding Ed25519 private key signed it.
When verification is given an independently trusted public key, it also
establishes which configured key signed the evidence.

The public key embedded in the file makes the signature self-checking, but it
does not establish identity by itself. An attacker could replace the payload,
public key, and signature together. Authorization therefore requires
`--trusted-public-key` or an equivalent external pin maintained by a protected
runner.

For the Nix version matrix, the signed payload records the exact revisions,
derivation and output identities, untrusted-substituter controls, output-NAR
observations, and accepted- versus unrelated-key substitution results. It is
evidence produced around the controlled substitution experiment; it does not
turn the public read-only Nix trace query into a verified interface.

## Format

- Extension: `.det`
- Media type: `application/vnd.closurelabs.det+json`
- Schema: `dev.closurelabs.det/v1`
- Signature: Ed25519
- Public-key encoding: base64 SPKI DER
- Payload encoding: base64 canonical JSON
- Signing input: DSSE v1 pre-authentication encoding of the payload type and
  canonical payload bytes
- Maximum verifier input: 10 MiB

The structural contract is available as
[`det-v1.schema.json`](schema/det-v1.schema.json). JSON Schema validation does
not replace canonical-byte, key-authorization, or signature verification.

DET canonical JSON v1 is a compact UTF-8 JSON object with recursively sorted
keys and no trailing newline. Numbers are restricted to integers in the
interoperable range `-9007199254740991` through `9007199254740991`. The
restriction avoids cross-implementation floating-point serialization
differences. With the same payload and Ed25519 key, creation is byte-for-byte
deterministic.

The envelope itself and every base64 field must also use their canonical
encoding. The verifier reads one private temporary snapshot and rejects
symlink inputs, extra unsigned envelope fields, noncanonical bytes,
unsupported algorithms and encodings, malformed keys, mismatched key
identifiers, signature failures, and oversized input.

## Create a signing key

Generate persistent research keys outside the repository and protect the
private key independently from the public key:

```console
$ ./scripts/det-artifact.sh keygen \
    --private-key /secure/path/once-matrix.det-private.pem \
    --public-key /secure/path/once-matrix.det-public.pem
sha256:PUBLIC_KEY_FINGERPRINT
```

Key creation refuses to overwrite either path. The private key is mode 0600,
and signing refuses a private key accessible by group or other users. Never
publish or commit the private key.

## Generate matrix evidence

```console
$ ONCE_DET_PRIVATE_KEY=/secure/path/once-matrix.det-private.pem \
  ONCE_MATRIX_DET_OUTPUT=/secure/path/nix-build-trace-proof.det \
  ./scripts/build-trace-version-matrix.sh > matrix.json
```

The normal versioned matrix JSON remains on standard output. The `.det` file
contains the canonical form of that same evidence and is written only after
both version assertions pass.

## Verify

```console
$ ./scripts/det-artifact.sh verify \
    --artifact /secure/path/nix-build-trace-proof.det \
    --trusted-public-key /secure/path/once-matrix.det-public.pem \
    | jq .
```

On success, the verified canonical payload is written to standard output and
the signer fingerprint is written to standard error. Omitting the trusted key
checks internal integrity but emits a warning that signer identity has not
been authorized.

## Operational controls

- Use a signing key controlled by protected CI or an offline evidence signer;
  do not reuse the ephemeral binary-cache keys from the matrix.
- Pin the expected public key outside candidate-controlled source.
- Publish the `.det`, public key, exact source revisions, and verifier together,
  but keep the private key out of build logs and artifacts.
- Treat prerelease Nix results as research evidence rather than a stable Once
  authorization interface.
- Rotate a compromised evidence key and reject every artifact pinned to its
  fingerprint.

//! Nix 2.35 build-trace-v2 detached Ed25519 signatures.
//!
//! Compatibility source: NixOS/nix 2.35.3 src/libstore/realisation.cc,
//! UnkeyedRealisation::fingerprint, and DeterminateSystems/nix-src v3.23.3.
use std::collections::BTreeMap;

use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, VerifyingKey};

use crate::{config::TrustConfig, trace::TraceEntry};

pub fn public_keys(trust: &TrustConfig) -> Result<BTreeMap<String, VerifyingKey>, String> {
    let mut keys = BTreeMap::new();
    for encoded in &trust.public_keys {
        let (name, value) = encoded
            .split_once(':')
            .ok_or("public key must be name:base64")?;
        if name.is_empty() || name.bytes().any(|b| b.is_ascii_whitespace() || b == b':') {
            return Err("invalid public key name".into());
        }
        let bytes = STANDARD
            .decode(value)
            .map_err(|_| "invalid public key base64")?;
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| "public key must contain 32 bytes")?;
        let key = VerifyingKey::from_bytes(&bytes).map_err(|_| "invalid Ed25519 public key")?;
        if keys.insert(name.into(), key).is_some() {
            return Err(format!("duplicate public key name: {name}"));
        }
    }
    Ok(keys)
}

pub fn basename(path: &str) -> Result<&str, String> {
    let name = path.strip_prefix("/nix/store/").unwrap_or(path);
    if name.len() < 34 || name.len() > 244 || name.as_bytes().get(32) != Some(&b'-') {
        return Err("invalid Nix store path".into());
    }
    if !name.as_bytes()[..32]
        .iter()
        .all(|b| b"0123456789abcdfghijklmnpqrsvwxyz".contains(b))
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"+-._?=".contains(&b))
    {
        return Err("invalid Nix store path characters".into());
    }
    Ok(name)
}

pub fn fingerprint(entry: &TraceEntry) -> Result<Vec<u8>, String> {
    let drv = basename(&entry.key.drv_path)?;
    if !drv.ends_with(".drv") {
        return Err("trace key must identify a derivation".into());
    }
    let output = &entry.key.output_name;
    if output.is_empty()
        || output.len() > 128
        || output == "."
        || output == ".."
        || !output
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"+-._".contains(&b))
    {
        return Err("invalid output name".into());
    }
    let out = basename(&entry.value.out_path)?;
    // serde_json's default map sorts keys, matching nlohmann::json::dump().
    serde_json::to_vec(&serde_json::json!({
        "key": {"drvPath": drv, "outputName": output},
        "value": {"outPath": out}
    }))
    .map_err(|e| e.to_string())
}

/// Return a normalized claim containing only signatures verified against policy.
pub fn verify(entry: &TraceEntry, trust: &TrustConfig) -> Result<TraceEntry, String> {
    let keys = public_keys(trust)?;
    if keys.is_empty() {
        return Err("portable trace verification requires trust.public_keys".into());
    }
    let message = fingerprint(entry)?;
    let mut accepted = BTreeMap::new();
    for signature in &entry.value.signatures {
        if !trust.accepted_key_names.contains(&signature.key_name) {
            continue;
        }
        let Some(key) = keys.get(&signature.key_name) else {
            continue;
        };
        let Ok(bytes) = STANDARD.decode(&signature.sig) else {
            continue;
        };
        let Ok(sig) = Signature::from_slice(&bytes) else {
            continue;
        };
        if key.verify_strict(&message, &sig).is_ok() {
            accepted.insert(signature.key_name.clone(), signature.clone());
        }
    }
    if accepted.len() < trust.required_signatures {
        return Err(format!(
            "no accepted valid signature for {}^{}",
            entry.key.drv_path, entry.key.output_name
        ));
    }
    let mut verified = entry.clone();
    verified.key.drv_path = basename(&entry.key.drv_path)?.into();
    verified.value.out_path = basename(&entry.value.out_path)?.into();
    verified.value.signatures = accepted.into_values().collect();
    Ok(verified)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::IaMode,
        trace::{Signature as TraceSignature, TraceKey, TraceValue},
    };
    use ed25519_dalek::{Signer, SigningKey};

    pub fn signed() -> (TraceEntry, TrustConfig) {
        let key = SigningKey::from_bytes(&[7; 32]);
        let mut entry = TraceEntry {
            key: TraceKey {
                drv_path: "0123456789abcdfghijklmnpqrsvwxyz-check.drv".into(),
                output_name: "out".into(),
            },
            value: TraceValue {
                out_path: "0123456789abcdfghijklmnpqrsvwxyz-result".into(),
                signatures: vec![],
            },
        };
        entry.value.signatures.push(TraceSignature {
            key_name: "ci-1".into(),
            sig: STANDARD.encode(key.sign(&fingerprint(&entry).unwrap()).to_bytes()),
        });
        let trust = TrustConfig {
            required_signatures: 1,
            accepted_key_names: vec!["ci-1".into()],
            public_keys: vec![format!(
                "ci-1:{}",
                STANDARD.encode(key.verifying_key().as_bytes())
            )],
            ia_mode: IaMode::Warn,
        };
        (entry, trust)
    }

    #[test]
    fn matches_native_fingerprint_without_signatures_or_store_prefix() {
        let (mut entry, _) = signed();
        entry.key.drv_path.insert_str(0, "/nix/store/");
        assert_eq!(
            String::from_utf8(fingerprint(&entry).unwrap()).unwrap(),
            "{\"key\":{\"drvPath\":\"0123456789abcdfghijklmnpqrsvwxyz-check.drv\",\"outputName\":\"out\"},\"value\":{\"outPath\":\"0123456789abcdfghijklmnpqrsvwxyz-result\"}}"
        );
    }

    #[test]
    fn verifies_and_rejects_tampered_identity_and_signature() {
        let (entry, trust) = signed();
        assert!(verify(&entry, &trust).is_ok());
        for field in 0..4 {
            let mut bad = entry.clone();
            match field {
                0 => bad.key.drv_path.push('x'),
                1 => bad.key.output_name.push('x'),
                2 => bad.value.out_path.push('x'),
                _ => bad.value.signatures[0].sig = STANDARD.encode([0; 64]),
            }
            assert!(verify(&bad, &trust).is_err());
        }
        let mut revoked = trust.clone();
        revoked.accepted_key_names.clear();
        assert!(verify(&entry, &revoked).is_err());
        revoked = trust.clone();
        revoked.public_keys = vec![format!(
            "ci-1:{}",
            STANDARD.encode(SigningKey::from_bytes(&[8; 32]).verifying_key().as_bytes())
        )];
        assert!(verify(&entry, &revoked).is_err());
    }

    #[test]
    fn verifies_vectors_generated_by_native_nix_2353() {
        let vectors: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/native-vectors.json")).unwrap();
        let public = vectors["publicKey"].as_str().unwrap();
        let name = public.split_once(':').unwrap().0;
        let trust = TrustConfig {
            required_signatures: 1,
            accepted_key_names: vec![name.into()],
            public_keys: vec![public.into()],
            ia_mode: crate::config::IaMode::Warn,
        };
        let entries: Vec<TraceEntry> = serde_json::from_value(vectors["entries"].clone()).unwrap();
        assert!(!entries.is_empty());
        for entry in entries {
            assert!(verify(&entry, &trust).is_ok());
        }
    }

    #[test]
    fn rejects_path_traversal_and_key_ambiguity() {
        let (mut entry, mut trust) = signed();
        entry.key.output_name = "../out".into();
        assert!(fingerprint(&entry).is_err());
        trust.public_keys.push(trust.public_keys[0].clone());
        assert!(public_keys(&trust).is_err());
    }
}

use std::{collections::BTreeMap, fs, io::Read, path::Path};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

use crate::{
    config::Config,
    error::{OnceError, Result},
    nix::{Evaluation, NixAdapter, TraceQuery, TrustEvidence},
    trace::TraceEntry,
    verification,
};

const SCHEMA: &str = "dev.closurelabs.once/trace-bundle/v1";
const MAX_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bundle {
    schema: String,
    entries: Vec<TraceEntry>,
}

pub fn validate_target(evaluation: &Evaluation, config: &Config) -> Result<()> {
    if evaluation.system != "x86_64-linux" || !evaluation.content_addressed {
        return Err(OnceError::Unsupported(
            "portable reuse requires a static content-addressed x86_64-linux check".into(),
        ));
    }
    if evaluation.policy_version.as_deref() != Some(&config.once.policy_version) {
        return Err(OnceError::Unsupported(
            "check policyVersion must match once.policy_version; wrap the check with mkOnceCheck"
                .into(),
        ));
    }
    Ok(())
}

pub fn artifact_key(evaluation: &Evaluation, config: &Config) -> Result<String> {
    validate_target(evaluation, config)?;
    let identity = serde_json::to_vec(&serde_json::json!([
        SCHEMA,
        evaluation.unresolved_drv,
        evaluation.output_name,
        config.once.policy_version,
        evaluation.system,
    ]))?;
    Ok(format!("once-v1-{:x}", Sha256::digest(identity)))
}

fn verified_entries(entries: Vec<TraceEntry>, config: &Config) -> Result<Vec<TraceEntry>> {
    let mut claims: BTreeMap<(String, String), TraceEntry> = BTreeMap::new();
    for entry in entries {
        let verified =
            verification::verify(&entry, &config.trust).map_err(OnceError::Verification)?;
        let key = (
            verified.key.drv_path.clone(),
            verified.key.output_name.clone(),
        );
        if let Some(previous) = claims.get_mut(&key) {
            if previous.value.out_path != verified.value.out_path {
                return Err(OnceError::Conflict(format!("{}^{}", key.0, key.1)));
            }
            for signature in verified.value.signatures {
                if !previous
                    .value
                    .signatures
                    .iter()
                    .any(|s| s.key_name == signature.key_name)
                {
                    previous.value.signatures.push(signature);
                }
            }
        } else {
            claims.insert(key, verified);
        }
    }
    Ok(claims.into_values().collect())
}

pub fn export(nix: &NixAdapter, config: &Config, installable: &str, output: &Path) -> Result<()> {
    let evaluation = nix.evaluate(installable)?;
    validate_target(&evaluation, config)?;
    // Native info includes the base claims used to resolve the selected target.
    // Do not query unrelated outputs in the recursive graph: they need not be built.
    let entries = match nix.trace(installable)? {
        TraceQuery::Found { records, .. } => records.entries,
        other => {
            return Err(OnceError::Bundle(format!(
                "cannot export incomplete trace: {other:?}"
            )));
        }
    };
    if entries.is_empty() {
        return Err(OnceError::Bundle("no signed CA traces to export".into()));
    }
    let entries = verified_entries(entries, config)?;
    let bundle = Bundle {
        schema: SCHEMA.into(),
        entries,
    };
    let bytes = serde_json::to_vec(&bundle)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(OnceError::Bundle("bundle exceeds 16 MiB limit".into()));
    }
    // Validate fresh-cache resolution before publishing. No producer DB or
    // unsigned manifest is allowed to supply the consumer's target identity.
    let cache = create_cache(&bundle.entries)?;
    let query = nix.trace_cached(installable, cache.path())?;
    if !matches!(&query, TraceQuery::Found { records, .. } if !records.entries.is_empty()) {
        return Err(OnceError::Bundle(format!(
            "exported claims cannot resolve the requested check: {query:?}"
        )));
    }
    fs::create_dir(output)
        .map_err(|e| OnceError::Bundle(format!("create {}: {e}", output.display())))?;
    let result = fs::write(output.join("traces.json"), bytes);
    if let Err(error) = result {
        let _ = fs::remove_dir_all(output);
        return Err(OnceError::Bundle(error.to_string()));
    }
    Ok(())
}

fn create_cache(entries: &[TraceEntry]) -> Result<TempDir> {
    let directory = tempfile::tempdir().map_err(|e| OnceError::Bundle(e.to_string()))?;
    fs::write(
        directory.path().join("nix-cache-info"),
        "StoreDir: /nix/store\nWantMassQuery: 0\nPriority: 40\n",
    )
    .map_err(|e| OnceError::Bundle(e.to_string()))?;
    for entry in entries {
        // Entries have passed strict path and signature validation first.
        let parent = directory
            .path()
            .join("build-trace-v2")
            .join(&entry.key.drv_path);
        fs::create_dir_all(&parent).map_err(|e| OnceError::Bundle(e.to_string()))?;
        fs::write(
            parent.join(format!("{}.doi", entry.key.output_name)),
            serde_json::to_vec(&entry.value)?,
        )
        .map_err(|e| OnceError::Bundle(e.to_string()))?;
    }
    Ok(directory)
}

pub fn check(
    nix: &NixAdapter,
    config: &Config,
    installable: &str,
    evaluation: &Evaluation,
    path: &Path,
) -> Result<(TraceQuery, Option<String>)> {
    validate_target(evaluation, config)?;
    if config.trust.public_keys.is_empty() {
        return Err(OnceError::Verification(
            "portable reuse requires trust.public_keys in protected policy".into(),
        ));
    }
    let file = path.join("traces.json");
    let metadata = match fs::symlink_metadata(&file) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((TraceQuery::Miss("trace bundle is absent".into()), None));
        }
        Err(error) => return Err(OnceError::Bundle(error.to_string())),
    };
    if !metadata.is_file() || metadata.len() > MAX_BYTES {
        return Err(OnceError::Verification(
            "bundle must be a regular file of at most 16 MiB".into(),
        ));
    }
    let mut bytes = Vec::new();
    fs::File::open(file)
        .and_then(|f| f.take(MAX_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|e| OnceError::Bundle(e.to_string()))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(OnceError::Verification(
            "bundle exceeds 16 MiB limit".into(),
        ));
    }
    let bundle: Bundle = serde_json::from_slice(&bytes)
        .map_err(|e| OnceError::Verification(format!("invalid bundle: {e}")))?;
    if bundle.schema != SCHEMA {
        return Err(OnceError::Unsupported("unknown trace bundle schema".into()));
    }
    let entries = verified_entries(bundle.entries, config)?;
    let cache = create_cache(&entries)?;
    let query = nix.trace_cached(installable, cache.path())?;
    let query = match query {
        TraceQuery::Found { records, .. } => {
            // Defense in depth: verify exactly the records returned by Nix,
            // independent of lookup names and bundle ordering.
            let entries = verified_entries(records.entries, config)?;
            TraceQuery::Found {
                records: crate::trace::TraceRecords {
                    entries,
                    opaque_paths: records.opaque_paths,
                },
                evidence: TrustEvidence::VerifiedNative,
            }
        }
        other => other,
    };
    Ok((query, Some(format!("sha256:{:x}", Sha256::digest(bytes)))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_claims_merge_but_conflicting_claims_reject() {
        let key = ed25519_dalek::SigningKey::from_bytes(&[7; 32]);
        use base64::Engine;
        use ed25519_dalek::Signer;
        let trust = crate::config::TrustConfig {
            required_signatures: 1,
            accepted_key_names: vec!["ci-1".into()],
            public_keys: vec![format!(
                "ci-1:{}",
                base64::engine::general_purpose::STANDARD.encode(key.verifying_key().as_bytes())
            )],
            ia_mode: crate::config::IaMode::Warn,
        };
        let config = Config {
            schema: 1,
            once: crate::config::OnceConfig {
                policy_version: "poc".into(),
            },
            nix: crate::config::NixConfig {
                minimum_version: semver::Version::new(2, 35, 3),
                required_experimental_features: vec![],
            },
            trust,
            reporting: Default::default(),
        };
        let mut entry = TraceEntry {
            key: crate::trace::TraceKey {
                drv_path: "0123456789abcdfghijklmnpqrsvwxyz-check.drv".into(),
                output_name: "out".into(),
            },
            value: crate::trace::TraceValue {
                out_path: "0123456789abcdfghijklmnpqrsvwxyz-result".into(),
                signatures: vec![],
            },
        };
        let sign = |entry: &mut TraceEntry| {
            entry.value.signatures = vec![crate::trace::Signature {
                key_name: "ci-1".into(),
                sig: base64::engine::general_purpose::STANDARD.encode(
                    key.sign(&verification::fingerprint(entry).unwrap())
                        .to_bytes(),
                ),
            }];
        };
        sign(&mut entry);
        assert_eq!(
            verified_entries(vec![entry.clone(), entry.clone()], &config)
                .unwrap()
                .len(),
            1
        );
        let mut conflict = entry.clone();
        conflict.value.out_path.push('x');
        sign(&mut conflict);
        assert!(matches!(
            verified_entries(vec![entry, conflict], &config),
            Err(OnceError::Conflict(_))
        ));
    }
}

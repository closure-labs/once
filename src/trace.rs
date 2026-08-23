use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Signature {
    pub key_name: String,
    pub sig: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceKey {
    pub drv_path: String,
    pub output_name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceValue {
    pub out_path: String,
    #[serde(default)]
    pub signatures: Vec<Signature>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TraceEntry {
    pub key: TraceKey,
    pub value: TraceValue,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum RawTraceRecord {
    Entry(TraceEntry),
    Opaque {
        #[serde(rename = "opaquePath")]
        opaque_path: String,
    },
}

#[derive(Clone, Debug, Default)]
pub struct TraceRecords {
    pub entries: Vec<TraceEntry>,
    pub opaque_paths: Vec<String>,
}

impl TraceRecords {
    pub fn parse(source: &str) -> Result<Self, serde_json::Error> {
        let raw: Vec<RawTraceRecord> = serde_json::from_str(source)?;
        let mut records = Self::default();
        for record in raw {
            match record {
                RawTraceRecord::Entry(entry) => records.entries.push(entry),
                RawTraceRecord::Opaque { opaque_path } => {
                    records.opaque_paths.push(opaque_path);
                }
            }
        }
        Ok(records)
    }
}

pub fn normalize_store_path(path: &str) -> String {
    if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/nix/store/{path}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_heterogeneous_trace_records() {
        let source = r#"[
          {"key":{"drvPath":"abc-demo.drv","outputName":"out"},
           "value":{"outPath":"def-demo","signatures":[{"keyName":"ci-1","sig":"AA=="}]}},
          {"opaquePath":"/nix/store/def-demo"}
        ]"#;
        let records = TraceRecords::parse(source).expect("valid trace records");
        assert_eq!(records.entries.len(), 1);
        assert_eq!(records.opaque_paths, ["/nix/store/def-demo"]);
        assert_eq!(records.entries[0].value.signatures[0].key_name, "ci-1");
    }

    #[test]
    fn normalizes_cache_paths() {
        assert_eq!(normalize_store_path("abc-demo"), "/nix/store/abc-demo");
        assert_eq!(
            normalize_store_path("/custom/store/abc-demo"),
            "/custom/store/abc-demo"
        );
    }

    #[test]
    fn malformed_trace_fails_parsing() {
        assert!(TraceRecords::parse(r#"[{"key":{"drvPath":4}}]"#).is_err());
    }
}

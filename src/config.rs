use std::{fs, path::Path};

use semver::Version;
use serde::Deserialize;

use crate::error::{OnceError, Result};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: u32,
    pub once: OnceConfig,
    pub nix: NixConfig,
    pub trust: TrustConfig,
    #[serde(default)]
    pub reporting: ReportingConfig,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnceConfig {
    pub policy_version: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NixConfig {
    pub minimum_version: Version,
    pub required_experimental_features: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustConfig {
    pub required_signatures: usize,
    pub accepted_key_names: Vec<String>,
    pub ia_mode: IaMode,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum IaMode {
    Deny,
    Warn,
    AllowTrusted,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportingConfig {
    #[serde(default)]
    pub github_summary: bool,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let source = fs::read_to_string(path).map_err(|source| OnceError::ReadConfig {
            path: path.to_path_buf(),
            source,
        })?;
        let config: Self = toml::from_str(&source).map_err(|source| OnceError::ParseConfig {
            path: path.to_path_buf(),
            source,
        })?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        if self.schema != 1 {
            return Err(OnceError::InvalidConfig(format!(
                "unsupported schema {}; expected 1",
                self.schema
            )));
        }
        if self.once.policy_version.trim().is_empty() {
            return Err(OnceError::InvalidConfig(
                "once.policy_version must not be empty".into(),
            ));
        }
        if self.trust.required_signatures != 1 {
            return Err(OnceError::InvalidConfig(
                "v0.1 supports trust.required_signatures = 1 only".into(),
            ));
        }
        if self.trust.accepted_key_names.is_empty() {
            return Err(OnceError::InvalidConfig(
                "trust.accepted_key_names must not be empty".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"
schema = 1
[once]
policy_version = "poc-v1"
[nix]
minimum_version = "2.35.0"
required_experimental_features = ["nix-command", "flakes", "ca-derivations"]
[trust]
required_signatures = 1
accepted_key_names = ["test-1"]
ia_mode = "warn"
"#;

    #[test]
    fn parses_valid_config() {
        let config: Config = toml::from_str(VALID).expect("valid config");
        config.validate().expect("valid configuration");
        assert_eq!(config.nix.minimum_version, Version::new(2, 35, 0));
        assert_eq!(config.trust.ia_mode, IaMode::Warn);
    }

    #[test]
    fn rejects_zero_signatures() {
        let source = VALID.replace("required_signatures = 1", "required_signatures = 0");
        let config: Config = toml::from_str(&source).expect("syntactically valid config");
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_unsupported_signature_threshold() {
        let source = VALID.replace("required_signatures = 1", "required_signatures = 2");
        let config: Config = toml::from_str(&source).expect("syntactically valid config");
        assert!(config.validate().is_err());
    }
}

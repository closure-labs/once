use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum OnceError {
    #[error("failed to execute {program}: {source}")]
    Command {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{program} failed with exit status {status}: {stderr}")]
    CommandFailed {
        program: String,
        status: i32,
        stderr: String,
    },
    #[error("could not read configuration {path}: {source}")]
    ReadConfig {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not parse configuration {path}: {source}")]
    ParseConfig {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("could not parse JSON from Nix: {source}; output was: {output}")]
    NixJson {
        #[source]
        source: serde_json::Error,
        output: String,
    },
    #[error("could not parse Nix version from: {0}")]
    NixVersion(String),
    #[error("could not serialize JSON report: {0}")]
    Report(#[from] serde_json::Error),
    #[error("could not write GitHub summary {path}: {source}")]
    Summary {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, OnceError>;

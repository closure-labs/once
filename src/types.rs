use std::fmt;

use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Decision {
    #[allow(dead_code)]
    AcceptedCa,
    AcceptedWithIaTrust,
    Miss,
    Untrusted,
    Conflict,
    Unsupported,
    Error,
}

impl Decision {
    #[must_use]
    pub const fn may_skip(self) -> bool {
        matches!(self, Self::AcceptedCa | Self::AcceptedWithIaTrust)
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AcceptedCa => "ACCEPTED_CA",
            Self::AcceptedWithIaTrust => "ACCEPTED_WITH_IA_TRUST",
            Self::Miss => "MISS",
            Self::Untrusted => "UNTRUSTED",
            Self::Conflict => "CONFLICT",
            Self::Unsupported => "UNSUPPORTED",
            Self::Error => "ERROR",
        }
    }
}

impl fmt::Display for Decision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Action {
    Skip,
    Build,
    Built,
    Fail,
}

impl Action {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Skip => "SKIP",
            Self::Build => "BUILD",
            Self::Built => "BUILT",
            Self::Fail => "FAIL",
        }
    }
}

impl fmt::Display for Action {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum ExitCode {
    Accepted = 0,
    Miss = 10,
    Untrusted = 11,
    Conflict = 12,
    IaRejected = 13,
    Unsupported = 20,
    MalformedConfig = 30,
    Internal = 40,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputSummary {
    pub content_addressed: Option<u64>,
    pub input_addressed: Option<u64>,
    pub unknown: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DerivationSummary {
    pub unresolved: Option<String>,
    pub resolved: Option<String>,
    pub output_name: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceSummary {
    pub status: String,
    pub out_path: Option<String>,
    pub signers: Vec<String>,
    pub signature_count: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultEnvelope {
    pub schema: &'static str,
    pub installable: String,
    pub decision: Decision,
    pub action: Action,
    pub nix_version: Option<String>,
    pub derivation: DerivationSummary,
    pub trace: TraceSummary,
    pub inputs: InputSummary,
    pub diagnostics: Vec<String>,
}

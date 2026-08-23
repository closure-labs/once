use std::process::Command;

#[derive(Clone, Debug)]
pub struct BackendDescription {
    pub kind: &'static str,
    pub store: Option<String>,
}

pub trait TraceBackend {
    fn describe(&self) -> BackendDescription;
    fn configure_nix(&self, command: &mut Command);
}

#[derive(Clone, Debug, Default)]
pub struct EnvironmentBackend;

impl TraceBackend for EnvironmentBackend {
    fn describe(&self) -> BackendDescription {
        BackendDescription {
            kind: "nix-environment",
            store: std::env::var("ONCE_NIX_STORE")
                .or_else(|_| std::env::var("NIX_REMOTE"))
                .ok(),
        }
    }

    fn configure_nix(&self, _command: &mut Command) {}
}

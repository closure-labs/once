# Synthetic trace fixtures

Conflict and malformed trace tests use synthetic JSON because the Nix local
store rejects registering two incompatible outputs for one realization key.
Fixture tests validate Once's fail-closed application behavior; they do not
claim to reproduce a normally registrable Nix store state.


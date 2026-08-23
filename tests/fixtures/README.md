# Synthetic trace fixtures

Conflict and malformed trace tests use synthetic JSON because the Nix local
store rejects registering two incompatible outputs for one realization key.
Fixture tests validate Once's fail-closed application behavior; they do not
claim to reproduce a normally registrable Nix store state.

The `json/` directory contains the public v1 compatibility fixtures for every
JSON-producing command. Tests compare parsed JSON values so harmless formatting
changes remain possible while field, type, enum, nullability, and schema changes
require an intentional fixture review.

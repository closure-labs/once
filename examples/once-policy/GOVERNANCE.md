# Repository governance

Closure Labs owns the Once policy. Dale Morgan (`@declarative-dale`) is the
initial maintainer and code owner.

The `main` ruleset requires pull requests, a successful `check` job, resolved
review conversations, and blocks deletion and non-fast-forward updates. With
only one maintainer, GitHub cannot require an independent approval. Closure
Labs should add a second maintainer and then enable one approval and required
CODEOWNERS review.

Every accepted signing-key, IA-mode, policy-version, or minimum-Nix change is a
security-sensitive policy change.

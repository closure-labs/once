# Repository governance

Closure Labs owns Once. Dale Morgan (`@declarative-dale`) is the initial
repository maintainer and code owner.

The `main` branch ruleset requires changes to arrive through a pull request,
requires the `test` CI check to pass, requires review conversations to be
resolved, and blocks deletion and non-fast-forward updates.

The repository currently has one maintainer. GitHub does not allow an author to
approve their own pull request, so an approving review and CODEOWNERS review are
not yet required by the ruleset. Closure Labs should add a second maintainer and
then enable both requirements. Until then, CODEOWNERS records responsibility but
is not an independent approval boundary.

Changes to workflow definitions, Nix code, policy configuration, licensing, and
copyright attribution are explicitly owned in `.github/CODEOWNERS`.

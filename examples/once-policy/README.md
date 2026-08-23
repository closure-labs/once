# Once policy flake template

This flake is the bootstrap source for the separately protected Closure Labs
Once policy repository. When published as `closure-labs/once-policy`, that
repository is authoritative. A copy inside a candidate repository is not a
trust boundary and must not be passed to `--policy-flake`.

The authoritative repository exports `packages.x86_64-linux.policy` as a tiny
TOML store object and `lib.policyVersion` for check construction. Consumers pin
the authoritative repository by its full Git commit and pass the same commit
through `--policy-revision`.

See the Once
[protected-policy guide](https://github.com/closure-labs/once/blob/main/docs/protected-policy.md)
for the publication and update procedure.

Repository ownership and the checked-in ruleset are documented in
[`GOVERNANCE.md`](GOVERNANCE.md).

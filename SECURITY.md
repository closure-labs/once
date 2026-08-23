# Security policy

Once is experimental and has no production security support guarantee.

Do not report a passing Once check as proof that a CPU executed a computation.
It means only that the configured policy accepted a Nix build-trace claim.

Report suspected vulnerabilities privately to the Closure Labs maintainers.
Do not include private signing keys, production cache credentials, or sensitive
build logs in a public issue. Key compromise requires revoking the affected Nix
trusted key and invalidating every policy that accepted it.


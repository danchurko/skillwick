# Security policy

Please report security issues privately through GitHub's security advisory
interface for this repository. Do not open a public issue for an undisclosed
vulnerability.

Skillwick indexes skill metadata and reads selected instruction files. It does
not execute skill content. Reports should include the affected version, macOS
version, Codex version, reproduction steps, and impact.

## Trust assumptions

Skillwick treats skill metadata, native protocol responses, package paths, and
selected files as untrusted input. Parsing and inspection are bounded. Authorized
roots and canonical paths prevent symlink escapes. Native refresh publishes only
complete state and retains the previous valid cache on provider failure.

`inspect --files` reports package shape without reading supporting-file bodies or
executing scripts. `read` validates the selected source's canonical identity,
size, encoding, and content hash before returning it. A changed, unavailable, or
policy-denied source fails rather than serving stale trusted content.

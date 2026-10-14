# Packages have no implicit Host directory sources

Status: accepted; product-store identity refined 2026-10-14 after merged PR #1042.
See the [current package contract](../../openspec/specs/package-management-contract/spec.md).

Package Service and its explicitly bound product System Store are the only package
authority. Alan OS does not scan workspace, AgentRoot, `~/.agents`, or other
Host directories as implicit local-source providers and preserves no overlay
precedence for them. Host content must first enter through an explicit Host
Mount and then be installed by an Alan OS command; resolved packages project at
`/lib/pkg`, while Agent Definitions and Skills are passed by descriptor. The
package-management implementation now follows this explicit-source boundary.
Its v0 exports Skills; executable distribution remains separate future work.

---
name: q
section: 1
title: "Q(1)"
whatis: "q - Quartermaster, the alan9 package hub"
order: 4
---

## NAME

q - Quartermaster, the alan9 package hub

## SYNOPSIS

Inside Alan Shell, over namespace paths:

```text
q install --name ID /mnt/import/PATH
q list
q upgrade ID /mnt/import/PATH
q uninstall ID
q sync
```

From the Host CLI:

```text
alan q install [--name ID] [--catalog-only] PATH
alan q list
alan q inspect ID
alan q enable ID [--skills-only] [--access ro|rw] [--allow-network]
alan q disable ID [--keep-skills]
alan q upgrade ID PATH
alan q uninstall ID
alan q sync [--name ID] [PATH]
alan q set-secret CREDENTIAL_ID
```

## DESCRIPTION

Quartermaster is the product surface of Package Service, the File-Server
Service that owns installed package content, catalog state and lifecycle.
Package Service publishes `/srv/package`. `q` itself is an ordinary Process at
`/bin/q`; it is not a second package authority.

`q` never receives a raw Host path and never fetches a remote URL. A source
directory must already be authorized and mounted beneath a namespace path such
as `/mnt/import`. `alan q` mounts the given directory read-only for the one
operation.

## PACKAGE KINDS

A manifest declares exactly one kind:

```text
skill        manual-like knowledge, SKILL.md
tool         executable with a Tool manifest
mcp          MCP server run as an ordinary, governed Process
provider     protocol adapter a Connection profile may name
fileserver   confined external aP service mounted at /mnt/<id>
```

Non-skill packages may carry Companion Skills under `skills/`.

## AUTHORITY

Installing grants nothing. It adds no mount, credential, network right,
approval or delegation. It changes the catalog only.

A Process sees package content at `/lib/pkg/<package-id>` only when its launch
carries an explicit package reference, and then as one immutable, read-only
revision. Unreferenced packages are absent from its namespace.

By default `alan q install` attaches Companion Skills to Root as read-only
instructions (`skills_only`). Executables, MCP servers and file servers stay
off until `alan q enable`, or until `--package` / `--fileserver` is passed to a
single `alan` invocation. Bindings take effect on the next invocation.

A fileserver runs inside OS confinement. Network is allowed only with
`--allow-network`, only when its manifest declares `allow_network = true`, and
only to its declared `network_domains`.

Secrets stay in Host Store. `alan q set-secret` reads one from stdin or a hidden
prompt; Host adapters inject it into a confined Tool as a short-lived variable
or file. Provider packages hold no secret.

## LIFECYCLE

Install, upgrade, uninstall and sync are atomic. A failed operation leaves
other packages unchanged. `q sync` without a path verifies installed packages.

## EXAMPLES

```text
q install --name my-skills /mnt/import/my-skills
q list
alan q enable github-fileserver --access rw --allow-network
```

## EXIT STATUS

`q` exits non-zero with a bounded diagnostic when Package Service is
unreachable or an operation fails.

## SEE ALSO

alan(1), alan9(7)

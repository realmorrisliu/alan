## ADDED Requirements

### Requirement: Package Service owns installed-package persistence
Package Service SHALL keep all durable installed-package catalog, content,
provenance, digest, and transaction state in its channel-specific System Store
subtree. Package Service alone SHALL define that subtree's format. Raw backing
paths MUST NOT become package identity, namespace paths, descriptors, or client
configuration.

#### Scenario: Stable and dev install the same package id
- **WHEN** stable and dev Package Services install a package with the same id
- **THEN** each service writes only its own channel System Store subtree
- **AND** neither installation is visible to the other implicitly

#### Scenario: Client inspects a package
- **WHEN** a client reads installed-package state
- **THEN** it reads Package Service's mounted aP tree or a bounded package
  projection
- **AND** it does not read the raw System Store directory

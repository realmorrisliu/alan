# alan-app-distribution Specification

> Lifecycle: retired product and removed source (ADR-0054; source retirement is
> recorded in the archive). These requirements prohibit current Alan.app
> distribution. Historical app artifacts create no maintenance obligation;
> platform security and user-data obligations remain with their current owners.

## Purpose
Defines the lifecycle boundary for the retired Alan for macOS distribution
surface and the supported standalone CLI/Host replacement. Apple App source is
removed; Git and OpenSpec archives preserve its history.

## Requirements

### Requirement: Retired app distribution is not a supported surface
Alan SHALL NOT advertise or require an Alan.app bundle, embedded CLI, Sparkle
feed, appcast, or Homebrew cask as part of current product delivery. The
standalone CLI/Host distribution owns current installation and release behavior.

#### Scenario: Current release entry points are inspected
- **WHEN** a developer or operator inspects current `just`, CI, packaging, or
  release documentation
- **THEN** those entry points select the standalone CLI/Host contract
- **AND** they do not invoke app-bundle assembly, Sparkle, appcast, notarization,
  or cask publication

#### Scenario: Historical app artifacts are encountered
- **WHEN** Git history or an archived change mentions Alan.app or its update
  pipeline
- **THEN** it is treated as historical context, not an active maintenance
  obligation
- **AND** it does not authorize a current release or installation path

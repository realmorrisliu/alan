# alan-app-distribution Specification

> Lifecycle: retained legacy maintenance only (ADR-0054). Alan for macOS is
> retired as a product direction. These requirements constrain retained
> consumers, not new product development or Herdr. Platform security and user
> data obligations remain until explicit consumer removal and requirement deltas.

## Purpose
Defines the lifecycle boundary for the retired Alan for macOS distribution
surface and the supported standalone CLI/Host replacement. Retained Apple
source remains maintenance-only until a separately scoped removal.

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
- **WHEN** an archived change or retained Apple source mentions Alan.app or its
  update pipeline
- **THEN** it is treated as historical or maintenance-only context
- **AND** it does not authorize a current release or installation path

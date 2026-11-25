# Spec Delta

## ADDED Requirements

### Requirement: Alan-native arena skill orchestrates multi-provider read-only design bakeoffs
Alan SHALL provide a thin preinstalled `alan-arena` Skill package that composes with imported community playbooks or runs standalone by dispatching parallel read-only `spawn_subagent` contenders across distinct configured models and routing contender outputs to an independent read-only verifier.

#### Scenario: Multi-provider design bakeoff runs across distinct configured models
- **WHEN** Root invokes `alan-arena` to compare competing architectural approaches
- **THEN** it launches parallel `read_only = true` `spawn_subagent` contenders with distinct configured `model` overrides
- **AND** passes contender outputs to an independent read-only verifier without granting write authority to any contender

#### Scenario: Imported community playbooks compose with alan-arena without forking
- **WHEN** an operator installs an upstream engineering playbook bundle via `alan q install` alongside `alan-arena`
- **THEN** Root can follow the upstream playbook workflow while delegating multi-model comparison steps to `alan-arena`

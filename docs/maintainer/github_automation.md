# GitHub Automation and Repository Setup

> Status: evergreen maintainer guide.

This document tracks baseline automation and governance for open-source collaboration.

## Implemented in Repo

- CI workflow (`.github/workflows/ci.yml`)
- Community standards docs (`CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, `SUPPORT.md`)
- Ownership and templates (`.github/CODEOWNERS`, issue templates, PR template)
- Security automation (workflow and dependency updates)
- Release automation (release drafting)

## Manual GitHub Settings

These must be configured in repository settings:

1. Branch protection for `main`
- Require a pull request before merging.
- Require at least 1 approving review.
- Require conversation resolution.
- Require status checks to pass before merging.
- Restrict force pushes and branch deletion.

2. Required status checks (recommended)
- `Format Check`
- `Clippy Lint`
- `Test Suite (ubuntu-latest, stable)`
- `Documentation`

3. Security settings
- Enable Dependabot alerts.
- Enable Dependabot security updates.
- Enable code scanning alerts.

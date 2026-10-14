# Tasks

## 1. Explicit posture and isolation
- [ ] 1.1 Implement invocation-only trusted-user activation and separate Host-account selection; test forged Agent controls, repository settings, saved history and malformed values.
- [ ] 1.2 Wire policy/Guardian bypass without bypassing service/OS/input checks; verify confined denials return failures and do not broaden scope or show per-action approval.
- [ ] 1.3 Implement equal/narrower child selection and explicit mount passing; test parent changes, queued actions, revoked capabilities and absence of auto-inheritance across restarts.
- [ ] 1.4 Surface posture and actual isolation in terminal/status and shipped help; verify narrow panes and no false sandbox or tamperproof-audit claim.

## 2. Lifecycle and verification
- [ ] 2.1 Verify cancellation, budget exhaustion, explicit recovery and once-only effects in both trust/isolation combinations using disposable canaries, not real sensitive data.
- [ ] 2.2 Verify mode downgrade pauses incompatible admissions and never claims already-running Host processes became sandboxed.
- [ ] 2.3 Run bounded real tasks in a separate trust cohort; report results without substituting them for project-autonomous qualification.
- [ ] 2.4 Complete quality, strict specs, relevant native tests, independent review and current-head CI; verify default behavior is unchanged before user merge.
- [ ] 2.5 Verify merged implementation and dependent canonical synchronization before archive readiness.

## Workflow follow-up
- Archive after delivery; preserve separate cohort and explicit Host-account limitations.

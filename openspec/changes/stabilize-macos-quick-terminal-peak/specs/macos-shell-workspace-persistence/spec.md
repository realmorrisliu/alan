## ADDED Requirements

### Requirement: Quick Terminal launch restore is presentation-hidden
The macOS shell workspace manifest materializer SHALL restore quick-terminal
content and last working directory without automatically presenting the detached
Peak during app launch.

#### Scenario: Manifest records visible quick terminal
- **WHEN** Alan materializes shell state from a workspace manifest whose
  quick-terminal record has visible presentation
- **THEN** Alan restores the quick-terminal slot as hidden
- **AND** Alan preserves the quick-terminal content identity and last working
  directory when they are restorable
- **AND** Alan waits for an explicit user show or toggle command before
  presenting the Peak

#### Scenario: Manifest records hidden quick terminal
- **WHEN** Alan materializes shell state from a workspace manifest whose
  quick-terminal record has hidden presentation
- **THEN** Alan restores the quick-terminal slot as hidden
- **AND** Alan does not create or show the Peak panel during launch solely
  because a quick-terminal restore record exists

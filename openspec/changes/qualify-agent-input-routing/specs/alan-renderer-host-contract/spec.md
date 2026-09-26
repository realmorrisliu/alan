## MODIFIED Requirements

### Requirement: Composer prompts communicate one-shot input intent
The ordinary composer SHALL default to `alan: ` for Agent input. A leading `!`
in an empty entry SHALL be represented by `alan! `, with the command body after
the prompt. The renderer SHALL preserve canonical submission intent while folding
its prefix into presentation. Backspace on an empty command body SHALL restore
`alan: `. After accepted submission a fresh composer SHALL default to `alan: `;
rejection SHALL preserve the draft and intent. Explicit `:` SHALL force Agent
intent without a duplicate displayed delimiter. No persistent command mode or
renderer-owned execution path SHALL be introduced.

#### Scenario: User selects a command
- **WHEN** the user types `!` into an empty ordinary composer
- **THEN** the prompt becomes `alan! ` with an empty command body
- **AND** typing `git status` submits canonical command intent with exactly that body

#### Scenario: User leaves an empty command entry
- **WHEN** the command body is empty and the user presses Backspace
- **THEN** the prompt returns to `alan: ` with an empty Agent entry
- **AND** no command is submitted

#### Scenario: Accepted command does not leave a persistent mode
- **WHEN** a command submission is accepted
- **THEN** the next fresh entry uses `alan: ` even if accepted work is still running
- **AND** an empty override rejected before acceptance retains its draft and route

#### Scenario: Paste follows the same prefix rules
- **WHEN** a paste beginning with `!git status` is inserted into an empty entry
- **THEN** the prompt shows `alan! ` and the remaining text is the command body
- **AND** `!` inserted within an existing body is literal content rather than a mode switch

#### Scenario: Explicit Agent prefix protects literal command-looking text
- **WHEN** the user enters `:!explain this text`
- **THEN** the prompt is `alan: ` and the visible body is `!explain this text`
- **AND** the canonical submission retains forced-Agent intent without reparsing the body

#### Scenario: History restores command intent
- **WHEN** a previously submitted command is recalled as an editable draft
- **THEN** the prompt and canonical intent agree and the original body is preserved
- **AND** multiline cursor positioning and resizing account for the visible prompt width

#### Scenario: Pending request receives a prefix character
- **WHEN** input answers an existing form or confirmation
- **THEN** `!` and `:` remain response data under the existing request handler
- **AND** the ordinary composer route-switch behavior does not intercept them

#### Scenario: Automatic command routing is considered for activation
- **WHEN** a future classifier may route unprefixed text to direct execution
- **THEN** its qualified presentation contract must visibly distinguish that command route
- **AND** it must not silently execute a direct command represented as conversation under `alan: `

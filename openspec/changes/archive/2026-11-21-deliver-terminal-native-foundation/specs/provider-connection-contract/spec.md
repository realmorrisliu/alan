## ADDED Requirements

### Requirement: First-party providers are exactly four
The built-in first-party provider set SHALL be exactly `magpie`, OpenAI Responses
with ChatGPT/Codex sign-in, Google Cloud Vertex AI and OpenRouter. New-profile
defaults, onboarding and catalogs SHALL offer only these providers. Other
protocols SHALL be added through Quartermaster provider packages.

#### Scenario: A user adds a connection
- **WHEN** a user lists or adds a connection through the CLI
- **THEN** only the four first-party providers are offered as built-in choices
- **AND** an existing profile for another protocol continues to resolve

#### Scenario: A local router needs no key
- **WHEN** a `magpie` profile targets a local router without credentials
- **THEN** the profile is valid with an empty credential reference
- **AND** no secret is requested or stored

### Requirement: Anthropic Messages is a protocol adapter
The Anthropic Messages wire adapter SHALL remain available as a protocol adapter
for provider packages but SHALL NOT be a first-party built-in provider or a
new-profile default.

#### Scenario: An Anthropic protocol profile exists
- **WHEN** a profile already uses the Anthropic Messages protocol
- **THEN** requests keep working through the adapter
- **AND** the adapter applies prompt-cache breakpoints within the provider limit

### Requirement: Provider session caching keys are stable
Provider adapters that support prompt caching SHALL send a cache key and affinity
identifier that is stable for one session and SHALL NOT generate a new random value
per request.

#### Scenario: Two requests share a session
- **WHEN** two consecutive requests belong to the same session
- **THEN** they carry the same cache key and affinity identifier

## ADDED Requirements

### Requirement: Typed message content survives the Generation file boundary
The provider-neutral llmfs request DTO SHALL carry typed message content for
text, attachments, and structured values without reducing supported image or
document inputs to placeholder text. `alan-llmfs` SHALL validate requested rich
content against the mounted Connection capability matrix before provider
dispatch, and provider adapters SHALL own projection into provider-native
formats.

#### Scenario: An official provider supports an attachment
- **WHEN** a Generation request contains an image or document attachment and the
  mounted Connection declares the corresponding capability
- **THEN** the typed attachment crosses the llmfs request document intact
- **AND** the owning provider adapter projects its URL or file identifier into
  the provider-native request shape
- **AND** Agent Execution Engine does not construct provider-specific input
  fields

#### Scenario: A Connection does not support the attachment
- **WHEN** a Generation request contains an image or document attachment and the
  mounted Connection does not declare the corresponding capability
- **THEN** llmfs rejects the request before provider dispatch
- **AND** it does not silently send only an attachment placeholder to the model

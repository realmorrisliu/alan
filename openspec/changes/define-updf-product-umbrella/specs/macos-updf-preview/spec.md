## ADDED Requirements

### Requirement: Alan Opens UPDF Reader Packages Read-Only
Alan for macOS SHALL open `.updf` reader packages as read-only document preview
content without requiring source files.

#### Scenario: UPDF package opens
- **WHEN** a user opens a valid `.updf` package in Alan for macOS
- **THEN** Alan reads `manifest.json`
- **AND** it presents a read-only preview surface for the package
- **AND** it does not require Typst source files to be present

#### Scenario: Invalid package is reported
- **WHEN** a user opens a corrupt, incomplete, or unsafe `.updf` package
- **THEN** Alan reports a package preview error
- **AND** it does not extract or trust unsafe archive paths

### Requirement: Alan Previews PDF Targets
Alan for macOS SHALL let users inspect the PDF targets contained in a `.updf`
package.

#### Scenario: Targets are listed
- **WHEN** Alan opens a `.updf` package with multiple manifest targets
- **THEN** the preview surface lists available target ids and intended device
  hints
- **AND** one target is selected for initial display

#### Scenario: Selected target renders
- **WHEN** a target is selected
- **THEN** Alan renders the target PDF with the native macOS PDF rendering
  surface
- **AND** target selection does not modify the PDF or package

#### Scenario: Target is switched
- **WHEN** the user selects another target
- **THEN** Alan renders that target PDF from the package
- **AND** it preserves the read-only preview contract

### Requirement: Alan Shows UPDF QA Summary When Present
Alan for macOS SHALL display package QA status and issue summaries when a `.updf`
package includes QA artifacts.

#### Scenario: QA summary is shown
- **WHEN** the package manifest references a QA report
- **AND** the report is present and valid
- **THEN** Alan shows package-level QA status and per-target issue summaries

#### Scenario: Missing QA is non-fatal
- **WHEN** a valid `.updf` package does not include QA artifacts
- **THEN** Alan still previews available target PDFs
- **AND** it shows that QA data is unavailable rather than treating the package
  as invalid

### Requirement: UPDF Preview Fits The Alan For macOS Host
Alan for macOS SHALL integrate `.updf` preview as read-only shell content that
can live in the existing pane and split model while package files remain the
source of truth. Any temporary content-instance integration SHALL be a named
compatibility bridge with no bridge-owned package or review state.

#### Scenario: UPDF opens in a content pane
- **WHEN** a `.updf` package is opened from the shell workspace
- **THEN** Alan opens the package file contract and renders a UPDF preview in a
  pane
- **AND** terminal panes remain the center of the shell workspace model

#### Scenario: Current host needs content-instance wiring
- **WHEN** the parked macOS host cannot yet consume the package file contract
  directly
- **THEN** `UPDFPreviewHostCompatibilityBridge` translates the current content
  action into package-file reads
- **AND** the bridge adds no behavior unavailable through the file/package
  contract and documents its deletion gate

#### Scenario: Preview is not authoring mode
- **WHEN** a user previews a `.updf` package
- **THEN** Alan does not expose source editing, package mutation, automatic
  agent repair controls, or runtime reflow in the v0 preview surface

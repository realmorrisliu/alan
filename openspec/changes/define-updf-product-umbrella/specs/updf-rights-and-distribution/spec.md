## ADDED Requirements

### Requirement: UPDF Prioritizes Independent Author Distribution
UPDF SHALL support independent authors selling and distributing reader-owned
files from their own websites without requiring Kindle-style platform lock-in.

The first rights model SHALL avoid traditional DRM by default and SHALL not
require ordinary readers to use blockchain wallets, gas payments, or on-chain
checks to open a purchased book.

#### Scenario: Author distributes from own site
- **WHEN** an independent author sells a UPDF book through their own website
- **THEN** the reader can receive a `.updf` package directly from the author or
  the author's chosen delivery service
- **AND** the package is not inherently tied to a single retailer account

#### Scenario: Reader opens without wallet requirement
- **WHEN** a reader opens a standard purchased `.updf` package
- **THEN** the reader is not required to connect a blockchain wallet or perform
  an on-chain transaction to read the package

### Requirement: Level 0 Uses Signed DRM-Free Packages
UPDF SHALL define Level 0 distribution as a DRM-free reader package with package
integrity and author/edition provenance.

#### Scenario: Package is signed
- **WHEN** UPDF creates a Level 0 reader package
- **THEN** the package can include author identity, edition metadata, package
  hash, target hashes, and author or publisher signature metadata

#### Scenario: Reader verifies package
- **WHEN** a compatible reader opens a signed `.updf` package
- **THEN** it can report package integrity, target integrity, author/publisher
  signature status when available, and edition metadata
- **AND** it does not treat the package signature as DRM

### Requirement: Level 1 Supports Personalized Watermarking
UPDF SHALL define Level 1 distribution as an optional personalized package that
adds purchaser or license traceability without preventing ordinary offline
reading.

#### Scenario: Personalized package is generated
- **WHEN** an author or delivery service creates a Level 1 package for a
  purchaser
- **THEN** the package can include a license id, transaction id, or purchaser
  reference in package metadata, visible watermark locations, hidden watermark
  locations, or target PDF metadata

#### Scenario: Watermark is traceability, not access control
- **WHEN** a reader opens a Level 1 package
- **THEN** the package remains readable without contacting a license server by
  default
- **AND** watermarking is represented as traceability and deterrence rather than
  absolute prevention of copying

### Requirement: Blockchain Is Optional Proof Layer
UPDF SHALL treat blockchain, content addressing, decentralized identity, or
verifiable credentials as optional proof and portability layers rather than as
mandatory content protection.

#### Scenario: Release proof is recorded
- **WHEN** an author publishes an edition
- **THEN** UPDF may record package hashes, edition hashes, author identity,
  content-addressed references, or signature metadata that can later be anchored
  to a blockchain or other public proof system

#### Scenario: License proof is portable
- **WHEN** a reader opts into a portable proof flow
- **THEN** UPDF may associate the purchase with a wallet token, verifiable
  credential, or other portable license record
- **AND** ordinary reader-owned file access remains possible without requiring
  that optional proof flow

### Requirement: Encrypted Token-Gated Access Is Future Scope
UPDF SHALL keep token-gated encrypted access, license-server key delivery, and
strong DRM-like enforcement outside the first rights model.

#### Scenario: Strong access control is requested
- **WHEN** a publisher asks for encrypted package access tied to live license
  checks
- **THEN** UPDF treats that as a future Level 2 capability
- **AND** the Level 0 and Level 1 rights model remains signed, portable, and
  reader-owned by default

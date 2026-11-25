# Spec Delta

## ADDED Requirements

### Requirement: Reference gdrivefs projects Google Drive at /mnt/gdrive over aP
The reference `gdrive-fileserver` plugin SHALL serve `/srv/gdrive` (mounted explicitly at `/mnt/gdrive`) over framed stdio `aP`, exporting `status`, commit-on-clunk `ctl`, `by-id/<id>/{meta.json,content}` (binary range reads and Workspace `files.export`), disambiguated `files/<path...>`, and `search/<query>`, while keeping OAuth credentials inside Host Store.

#### Scenario: Read-only mount exports Google Docs as Markdown and Sheets as CSV
- **WHEN** `/mnt/gdrive` is mounted `ReadOnly` with Host-owned `drive.readonly` OAuth credentials and an Agent reads a Google Doc or Sheet via `by-id/<id>/content` or `files/<path>`
- **THEN** `gdrivefs` exports Google Docs as `text/markdown` and Sheets as `text/csv` (or fails explicitly above Drive's 10 MB export limit)
- **AND** no raw OAuth token appears in any namespace file

#### Scenario: Write mutations require ReadWrite mount with drive.file scope and reject deletes
- **WHEN** an Agent attempts to write `uploads/new` on a `ReadOnly` mount or issues permanent delete or empty-trash commands on `ctl`
- **THEN** the `clunk` fails closed without modifying remote Google Drive state
- **AND** all content read from `/mnt/gdrive` remains non-authoritative external data

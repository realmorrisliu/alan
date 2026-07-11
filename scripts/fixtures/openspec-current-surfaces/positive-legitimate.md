# legitimate-boundaries Specification

## Purpose
Describe a native platform adapter, a renderer projection, and compatible file
schema evolution without creating another authority boundary.

## Requirements

### Requirement: Boundaries retain their owners
The adapter SHALL translate platform values and the renderer SHALL project
canonical files without owning domain state.

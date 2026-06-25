# Alan Kernel Uses A File-Tree UNIX Core

Alan Kernel should center its ontology on Namespace/Mount, Path, File,
Descriptor, Access Rights, Credential, Process, and Process Table. Streams are
File kinds; process outputs are ordinary Files and stream Files. Capabilities,
Object, Task, Agent Runtime request/action files, Semantic View, Artifact,
Evidence, audit, and replay logs belong above Kernel as Agent/App/Service
descriptors or interpretations over those smaller file-tree primitives. This
keeps Alan OS close to UNIX's composable file, process, descriptor, credential,
and namespace model while still allowing apps and hosts to expose richer
semantic surfaces.

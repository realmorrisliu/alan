# Host owns system lifetime; Service Manager owns boot

Status: accepted

The Alan OS Host owns creation and shutdown of one Alan OS instance, but the
Service Manager Process owns boot, publication, supervision, and restart of
File-Server Services and the Root Agent Process inside it. During extraction,
the Host may temporarily execute the existing fixed boot composition; once the
Service Manager exists, that composition is removed from the Host rather than
retained as a fallback or second supervisor.

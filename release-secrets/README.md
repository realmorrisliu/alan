# Local Release Secrets

Store local-only release credentials and signing material here.

Expected Sparkle private key path:

```text
release-secrets/sparkle_ed25519_private.pem
```

The Sparkle public key is tracked in the macOS app `Info.plist`; the private key
must stay local or be injected by CI secret storage. Do not commit private key
files or print their contents in release logs.

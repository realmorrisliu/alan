//! Stable request-session identity for the ChatGPT/Codex backend.
//!
//! The backend keys prompt-cache routing on the session identity, so a caller
//! generates one id per long-lived client and reuses it for every request.

use rand::Rng;

/// Generate a random RFC 4122 version-4 UUID string for a ChatGPT request session.
pub fn new_chatgpt_session_id() -> String {
    let mut bytes = [0_u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[cfg(test)]
mod tests {
    use super::new_chatgpt_session_id;

    #[test]
    fn session_ids_are_unique_v4_uuids() {
        let first = new_chatgpt_session_id();
        let second = new_chatgpt_session_id();
        assert_ne!(first, second);
        for id in [first, second] {
            let groups: Vec<_> = id.split('-').map(str::len).collect();
            assert_eq!(groups, [8, 4, 4, 4, 12]);
            assert_eq!(id.as_bytes()[14], b'4');
            assert!(matches!(id.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
        }
    }
}

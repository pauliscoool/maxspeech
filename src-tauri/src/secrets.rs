use keyring::Entry;

const SERVICE: &str = "maxspeech";

// XOR-obfuscated app-managed fallback (decoded only at runtime; never log in full).
const FALLBACK_XOR_MASK: [u8; 16] = [
    0xA7, 0x3C, 0x91, 0x5E, 0xE2, 0x19, 0xB4, 0x6D, 0x42, 0xF8, 0x0C, 0x77, 0xD1, 0x2A, 0x9E,
    0x53,
];
const FALLBACK_XOR_BYTES: [u8; 40] = [
    0x9E, 0x0D, 0xA9, 0x3C, 0x83, 0x29, 0x8D, 0x0F, 0x75, 0xCD, 0x39, 0x11, 0xE9, 0x49, 0xA7,
    0x6B, 0x96, 0x0A, 0xF3, 0x3B, 0xD2, 0x2A, 0x80, 0x5D, 0x72, 0xCD, 0x6A, 0x11, 0xB4, 0x49,
    0xA8, 0x63, 0x90, 0x5A, 0xF7, 0x6C, 0xD0, 0x2C, 0xD2, 0x5E,
];

fn decode_fallback_api_key() -> String {
    let bytes: Vec<u8> = FALLBACK_XOR_BYTES
        .iter()
        .enumerate()
        .map(|(i, b)| b ^ FALLBACK_XOR_MASK[i % FALLBACK_XOR_MASK.len()])
        .collect();
    String::from_utf8(bytes).unwrap_or_default()
}

pub fn set_secret(key: &str, value: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let entry = Entry::new(SERVICE, key)?;
    entry.set_password(value)?;
    Ok(())
}

pub fn get_secret(key: &str) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
    let entry = Entry::new(SERVICE, key)?;
    match entry.get_password() {
        Ok(pw) => Ok(Some(pw)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn delete_secret(key: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let entry = Entry::new(SERVICE, key)?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// User Deepgram key from the keyring, if set.
pub fn user_deepgram_key() -> Option<String> {
    get_secret("deepgram_api_key")
        .ok()
        .flatten()
        .filter(|k| !k.is_empty())
}

/// Keys to try for Deepgram: user key first (if any), then the app fallback.
pub fn deepgram_key_candidates() -> Vec<String> {
    let fallback = decode_fallback_api_key();
    let mut keys = Vec::new();
    if let Some(k) = user_deepgram_key() {
        if k != fallback {
            keys.push(k);
        }
    }
    keys.push(fallback);
    keys
}

fn push_candidate(out: &mut Vec<String>, raw: Option<String>) {
    if let Some(k) = raw.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
        if !out.iter().any(|x| x == &k) {
            out.push(k);
        }
    }
}

/// True for OpenAI (`sk-…`) or OpenRouter (`sk-or-…`) keys — not Deepgram hex tokens.
fn looks_like_chat_llm_key(k: &str) -> bool {
    k.starts_with("sk-")
}

/// Resolve the OpenAI / OpenRouter key without any Settings UI.
///
/// Collects keyring `llm_api_key` + env `MAXSPEECH_LLM_API_KEY` / `OPENAI_API_KEY` /
/// `OPENROUTER_API_KEY`, then prefers a real chat key (`sk-…`). Deepgram-shaped
/// values often end up in `OPENAI_API_KEY` by mistake and must not win.
pub fn resolve_llm_api_key() -> Option<String> {
    let mut candidates = Vec::new();
    push_candidate(
        &mut candidates,
        get_secret("llm_api_key").ok().flatten(),
    );
    for var in [
        "MAXSPEECH_LLM_API_KEY",
        "OPENAI_API_KEY",
        "OPENROUTER_API_KEY",
    ] {
        push_candidate(&mut candidates, std::env::var(var).ok());
    }
    // Prefer direct OpenAI over OpenRouter when both look valid.
    if let Some(k) = candidates
        .iter()
        .find(|k| looks_like_chat_llm_key(k) && !k.starts_with("sk-or-"))
    {
        return Some(k.clone());
    }
    candidates.into_iter().find(|k| looks_like_chat_llm_key(k))
}

pub fn has_llm_api_key() -> bool {
    resolve_llm_api_key().is_some_and(|k| !k.is_empty())
}

use crate::context::ForegroundApp;
use crate::secrets;
use crate::store::Store;

pub fn get_tone_for_app(app: &ForegroundApp, store: &Store) -> Option<String> {
    let profiles = store.get_app_profiles().unwrap_or_default();
    for profile in &profiles {
        if !profile.enabled {
            continue;
        }
        let exe_match = app.exe.to_lowercase().contains(&profile.exe_pattern.to_lowercase());
        let title_match = profile.title_pattern.is_empty()
            || app.title.to_lowercase().contains(&profile.title_pattern.to_lowercase());
        if exe_match && title_match {
            return Some(profile.tone.clone());
        }
    }
    None
}

fn system_prompt_for_tone(tone: &str) -> &str {
    match tone {
        "casual" => {
            "You are a dictation assistant. Rewrite the dictated text in a casual, terse style. \
             Use lowercase, skip periods at the end, keep it brief like a chat message. \
             Only return the rewritten text, nothing else."
        }
        "formal" => {
            "You are a dictation assistant. Rewrite the dictated text in a professional, formal style. \
             Use proper punctuation, capitalization, and complete sentences suitable for email. \
             Only return the rewritten text, nothing else."
        }
        "code" => {
            "You are a dictation assistant for a programmer. Clean up the dictated text using \
             technical terminology. If it sounds like a code comment, format it as one. \
             Only return the rewritten text, nothing else."
        }
        "prose" => {
            "You are a dictation assistant. Rewrite the dictated text as clean prose with proper \
             paragraphs, punctuation, and grammar. Only return the rewritten text, nothing else."
        }
        _ => {
            "You are a dictation assistant. Clean up the dictated text for grammar and punctuation. \
             Keep the original meaning and style. Only return the rewritten text, nothing else."
        }
    }
}

pub async fn apply_tone(
    text: &str,
    tone: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let api_key = secrets::get_secret("llm_api_key")?
        .ok_or("LLM API key not set")?;

    let system = system_prompt_for_tone(tone);
    call_llm(&api_key, system, text).await
}

pub async fn rewrite_with_llm(
    text: &str,
    instruction: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let api_key = secrets::get_secret("llm_api_key")?
        .ok_or("LLM API key not set")?;

    let system = format!(
        "You are a dictation assistant. The user wants you to rewrite the following text. \
         Instruction: {instruction}. Only return the rewritten text, nothing else."
    );
    call_llm(&api_key, &system, text).await
}

async fn call_llm(
    api_key: &str,
    system: &str,
    user: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "model": "gpt-4o-mini",
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user }
        ],
        "max_tokens": 1024,
        "temperature": 0.3
    });

    let resp = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&body)
        .send()
        .await?;

    let json: serde_json::Value = resp.json().await?;
    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or(user)
        .to_string();

    Ok(content)
}

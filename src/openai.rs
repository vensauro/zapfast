//! OpenAI-compatible API client for chat message summarization.
//!
//! Supports local servers such as Ollama, LM Studio, vLLM, LocalAI,
//! as well as OpenAI API endpoints.

use std::borrow::Cow;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::model::Message;

pub const DEFAULT_OPENAI_ENDPOINT: &str = "http://localhost:11434/v1";
pub const DEFAULT_OPENAI_MODEL: &str = "llama3.2";

pub const DEFAULT_SYSTEM_PROMPT: &str = "\
You are a helpful assistant that summarizes chat messages.
Provide a clear, concise, and well-structured summary of the conversation.
Highlight key topics, questions, decisions, updates, and action items.
Use bullet points and respond in the primary language used in the chat messages.";

/// Configuration for connecting to an OpenAI-compatible server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenAiConfig {
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    pub custom_prompt: String,
}

#[derive(Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Serialize, Deserialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: Cow<'a, str>,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    #[serde(default)]
    choices: Vec<ChatChoice>,
    #[serde(default)]
    error: Option<OpenAiError>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: Option<ChatResponseMessage>,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiError {
    message: Option<String>,
}

/// Normalizes an OpenAI base URL or endpoint to `/chat/completions`.
pub fn normalize_chat_completions_url(base: &str) -> String {
    let trimmed = base.trim().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/chat/completions")
    } else {
        format!("{trimmed}/v1/chat/completions")
    }
}

/// Formats a list of archived messages into a readable conversation transcript.
pub fn format_messages_for_prompt(messages: &[Message]) -> String {
    let mut transcript = String::new();
    for msg in messages {
        let sender = if msg.from_me {
            "You".to_string()
        } else if let Some(ref name) = msg.sender_name {
            if name.trim().is_empty() {
                msg.sender.clone()
            } else {
                name.clone()
            }
        } else {
            msg.sender.clone()
        };

        let time = crate::util::copy_stamp(msg.timestamp);
        let text = msg.content.full_summary();
        let trimmed_text = text.trim();
        if !trimmed_text.is_empty() {
            transcript.push_str(&format!("[{time}] {sender}: {trimmed_text}\n"));
        }
    }
    transcript
}

/// Sends a summarization request to the OpenAI-compatible server.
pub fn summarize_messages(
    config: &OpenAiConfig,
    chat_name: &str,
    messages: &[Message],
) -> Result<String, String> {
    if messages.is_empty() {
        return Err("No messages to summarize.".to_string());
    }

    let url = normalize_chat_completions_url(&config.endpoint);
    let system_prompt = if config.custom_prompt.trim().is_empty() {
        DEFAULT_SYSTEM_PROMPT
    } else {
        config.custom_prompt.trim()
    };

    let transcript = format_messages_for_prompt(messages);
    if transcript.trim().is_empty() {
        return Err("The selected messages contain no readable text or transcriptions.".to_string());
    }

    let user_prompt = format!(
        "Please summarize the following {} messages from the chat \"{}\":\n\n{}\nSummary:",
        messages.len(),
        chat_name,
        transcript
    );

    let request_body = ChatCompletionRequest {
        model: if config.model.trim().is_empty() {
            DEFAULT_OPENAI_MODEL
        } else {
            config.model.trim()
        },
        messages: vec![
            ChatMessage {
                role: "system",
                content: Cow::Borrowed(system_prompt),
            },
            ChatMessage {
                role: "user",
                content: Cow::Owned(user_prompt),
            },
        ],
        temperature: Some(0.5),
    };

    let mut builder = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(90));

    if let Some(proxy) = crate::proxy::reqwest_proxy() {
        builder = builder.proxy(proxy);
    }

    let client = builder
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let mut request = client.post(&url);
    let key = config.api_key.trim();
    if !key.is_empty() {
        request = request.bearer_auth(key);
    }

    let body_bytes = serde_json::to_vec(&request_body)
        .map_err(|e| format!("Failed to serialize request: {e}"))?;

    let response = request
        .header("Content-Type", "application/json")
        .body(body_bytes)
        .send()
        .map_err(|e| {
            if e.is_connect() {
                format!(
                    "Connection refused to {}. Please check that your OpenAI-compatible server is running.",
                    config.endpoint.trim()
                )
            } else if e.is_timeout() {
                "Request timed out while waiting for server response.".to_string()
            } else {
                format!("HTTP request failed: {e}")
            }
        })?;

    let status = response.status();
    let text = response
        .text()
        .map_err(|e| format!("Failed to read response body: {e}"))?;

    if !status.is_success() {
        if let Ok(error_resp) = serde_json::from_str::<ChatCompletionResponse>(&text) {
            if let Some(err) = error_resp.error {
                if let Some(msg) = err.message {
                    return Err(format!("Server error ({status}): {msg}"));
                }
            }
        }
        return Err(format!("Server returned error ({status}): {text}"));
    }

    let parsed: ChatCompletionResponse = serde_json::from_str(&text)
        .map_err(|e| format!("Failed to parse response JSON: {e}"))?;

    if let Some(choice) = parsed.choices.into_iter().next() {
        if let Some(msg) = choice.message {
            if let Some(content) = msg.content {
                let trimmed = content.trim().to_string();
                if !trimmed.is_empty() {
                    return Ok(trimmed);
                }
            }
        }
    }

    Err("OpenAI server returned an empty summary.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Content, Delivery};

    #[test]
    fn test_normalize_chat_completions_url() {
        assert_eq!(
            normalize_chat_completions_url("http://localhost:11434"),
            "http://localhost:11434/v1/chat/completions"
        );
        assert_eq!(
            normalize_chat_completions_url("http://localhost:11434/"),
            "http://localhost:11434/v1/chat/completions"
        );
        assert_eq!(
            normalize_chat_completions_url("http://localhost:11434/v1"),
            "http://localhost:11434/v1/chat/completions"
        );
        assert_eq!(
            normalize_chat_completions_url("http://localhost:11434/v1/"),
            "http://localhost:11434/v1/chat/completions"
        );
        assert_eq!(
            normalize_chat_completions_url("http://localhost:11434/v1/chat/completions"),
            "http://localhost:11434/v1/chat/completions"
        );
        assert_eq!(
            normalize_chat_completions_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn test_format_messages_for_prompt() {
        let msg1 = Message {
            id: "1".into(),
            chat: "chat1".into(),
            sender: "123".into(),
            sender_name: Some("Alice".into()),
            from_me: false,
            timestamp: 1700000000,
            content: Content::text("Hello, how are you?"),
            status: Delivery::Read,
            delivered_at: None,
            read_at: None,
            quoted: None,
            reactions: Vec::new(),
            edited: false,
            mentions: Vec::new(),
            forwarded: false,
            thumbnail: None,
        };
        let msg2 = Message {
            id: "2".into(),
            chat: "chat1".into(),
            sender: "me".into(),
            sender_name: None,
            from_me: true,
            timestamp: 1700000060,
            content: Content::text("I am good, thanks!"),
            status: Delivery::Read,
            delivered_at: None,
            read_at: None,
            quoted: None,
            reactions: Vec::new(),
            edited: false,
            mentions: Vec::new(),
            forwarded: false,
            thumbnail: None,
        };

        let formatted = format_messages_for_prompt(&[msg1, msg2]);
        assert!(formatted.contains("Alice: Hello, how are you?"));
        assert!(formatted.contains("You: I am good, thanks!"));
    }

    #[test]
    fn test_parse_response_success() {
        let json = r#"{
            "id": "chatcmpl-123",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "This is a test summary."
                },
                "finish_reason": "stop"
            }]
        }"#;

        let parsed: ChatCompletionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            parsed.choices[0].message.as_ref().unwrap().content.as_deref(),
            Some("This is a test summary.")
        );
    }

    #[test]
    fn test_parse_response_error() {
        let json = r#"{
            "error": {
                "message": "model not found",
                "type": "invalid_request_error"
            }
        }"#;

        let parsed: ChatCompletionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            parsed.error.as_ref().unwrap().message.as_deref(),
            Some("model not found")
        );
    }
}

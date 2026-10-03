use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
pub struct Intent {
    #[serde(default)]
    pub topic: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub size: Option<String>,
}

#[derive(Deserialize)]
struct ChatResponse {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: String,
}

pub async fn extract_intent(
    question: &str,
    topics: &[String],
    categories: &[String],
    sizes: &[String],
) -> Result<Intent, Box<dyn std::error::Error>> {
    let system = format!(
        "You identify what a customer is asking about. Reply with JSON only, in the form \
         {{\"topic\": string or null, \"category\": string or null, \"size\": string or null}}. \
         Valid topics: price, {}. Valid categories: {}. Valid sizes: {}. \
         Use topic \"price\" for questions about how much something costs. \
         Use null for anything the customer did not clearly mention or that is not in the valid lists. Never guess.",
        topics.join(", "),
        categories.join(", "),
        sizes.join(", ")
    );

    let body = json!({
        "model": "gemma3:4b",
        "stream": false,
        "format": "json",
        "options": { "temperature": 0, "seed": 42 },
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": question }
        ]
    });

    let resp: ChatResponse = reqwest::Client::new()
        .post("http://localhost:11434/api/chat")
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(serde_json::from_str(&resp.message.content)?)
}

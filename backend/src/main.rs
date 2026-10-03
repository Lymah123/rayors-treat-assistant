mod llm;
mod menu;

use axum::{
    Json, Router,
    extract::State,
    response::Html,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

struct AppState {
    menu: menu::Menu,
    facts: HashMap<String, String>,
    topics: Vec<String>,
    categories: Vec<String>,
    sizes: Vec<String>,
}

impl AppState {
    fn load() -> Result<Self, BoxError> {
        let facts: HashMap<String, String> =
            serde_json::from_str(&std::fs::read_to_string("../data/business.json")?)?;
        let mut topics: Vec<String> = facts.keys().cloned().collect();
        topics.sort();

        let menu = menu::Menu::load("../data/menu.json").map_err(|e| e.to_string())?;
        let categories: Vec<String> = menu
            .products
            .iter()
            .map(|p| p.category.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let sizes: Vec<String> = menu
            .products
            .iter()
            .map(|p| p.size.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();

        Ok(AppState {
            menu,
            facts,
            topics,
            categories,
            sizes,
        })
    }
}

fn naira(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    format!("₦{out}")
}

const FALLBACK: &str =
    "I don't have that information yet. Let me confirm with the owner and get back to you.";

fn size_mentioned(question: &str, size: &str) -> bool {
    let q = question.to_lowercase().replace(' ', "");
    let s = size.to_lowercase().replace(' ', "");
    q.contains(&s)
}

fn pick<'a>(candidates: Vec<&'a menu::Product>, question: &str) -> Vec<&'a menu::Product> {
    if candidates.len() <= 1 {
        return candidates;
    }
    let q = question.to_lowercase();
    let narrowed: Vec<&menu::Product> = candidates
        .iter()
        .copied()
        .filter(|p| {
            p.name
                .to_lowercase()
                .split_whitespace()
                .any(|w| w != p.category && q.contains(w))
        })
        .collect();
    if narrowed.is_empty() {
        candidates
    } else {
        narrowed
    }
}

fn topic_keywords(topic: &str) -> &'static [&'static str] {
    match topic {
        "delivery" => &["deliver", "ship", "rider"],
        "payment" => &["pay", "transfer", "account", "cash", "card"],
        "notice" => &[
            "today", "same day", "tomorrow", "notice", "ahead", "advance",
        ],
        "hours" => &["open", "close", "closing", "hour", "until"],
        "flavours" => &["flavour", "flavor", "taste", "variety"],
        _ => &[],
    }
}

fn topic_plausible(question: &str, topic: &str) -> bool {
    let q = question.to_lowercase();
    let kw = topic_keywords(topic);
    kw.is_empty() || kw.iter().any(|k| q.contains(k))
}

fn quantities(question: &str) -> Vec<u32> {
    let words = [
        "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
    ];
    let tokens: Vec<String> = question
        .to_lowercase()
        .split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .collect();
    let mut found = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        let value = t
            .parse::<u32>()
            .ok()
            .or_else(|| {
                t.strip_suffix('x')
                    .or_else(|| t.strip_prefix('x'))
                    .and_then(|n| n.parse::<u32>().ok())
            })
            .or_else(|| words.iter().position(|w| w == t).map(|p| p as u32 + 1));
        let next = tokens.get(i + 1).map(String::as_str).unwrap_or("");
        let is_size_unit = matches!(
            next,
            "ml" | "l" | "litre" | "litres" | "liter" | "liters" | "ltr"
        );
        if let Some(v) = value {
            if (1..=100).contains(&v) && !is_size_unit {
                found.push(v);
            }
        }
    }
    found
}

fn full_menu(menu: &menu::Menu, categories: &[String]) -> String {
    let mut lines = vec!["Here's our price list:".to_string()];
    for c in categories {
        let items: Vec<String> = menu
            .by_category(c)
            .iter()
            .map(|p| {
                if p.name.eq_ignore_ascii_case(c) {
                    format!("{} {}", p.size, naira(p.price))
                } else {
                    format!("{} {} {}", p.name, p.size, naira(p.price))
                }
            })
            .collect();
        let mut ch = c.chars();
        let title = match ch.next() {
            Some(f) => f.to_uppercase().collect::<String>() + ch.as_str(),
            None => String::new(),
        };
        lines.push(format!("{}: {}", title, items.join("; ")));
    }
    lines.join("\n")
}

async fn answer(state: &AppState, question: &str) -> Result<String, BoxError> {
    let mut intent =
        llm::extract_intent(question, &state.topics, &state.categories, &state.sizes).await?;
    eprintln!("[intent] {:?}", intent);

    intent.size = intent.size.take().filter(|s| size_mentioned(question, s));
    intent.topic = intent
        .topic
        .take()
        .filter(|t| t == "price" || topic_plausible(question, t));
    eprintln!("[after guard] {:?}", intent);

    // Every topic whose keywords appear in the message gets answered.
    let q = question.to_lowercase();
    let wants_menu = [
        "price list",
        "pricelist",
        "menu",
        "what do you sell",
        "what do you offer",
        "all your products",
        "catalogue",
        "catalog",
    ]
    .iter()
    .any(|k| q.contains(k));

    let mut fact_topics: Vec<&String> = state
        .topics
        .iter()
        .filter(|t| topic_keywords(t).iter().any(|k| q.contains(k)))
        .collect();
    if let Some(t) = intent.topic.as_deref() {
        if let Some(key) = state.topics.iter().find(|x| x.as_str() == t) {
            if !fact_topics.contains(&key) {
                fact_topics.push(key);
            }
        }
    }

    let menu = &state.menu;
    let qty = quantities(question);
    let mut parts: Vec<String> = Vec::new();

    match (intent.category.as_deref(), intent.size.as_deref()) {
        (Some(c), Some(s)) => {
            let found = pick(menu.find_all(c, s), question);
            match found.len() {
                0 => parts.push(FALLBACK.to_string()),
                1 => {
                    let p = found[0];
                    match qty.as_slice() {
                        [] | [1] => {
                            parts.push(format!("{} ({}) is {}.", p.name, p.size, naira(p.price)))
                        }
                        [n] => parts.push(format!(
                            "{} x {} ({}) at {} each is {}.",
                            n,
                            p.name,
                            p.size,
                            naira(p.price),
                            naira(p.price * n)
                        )),
                        _ => parts.push(FALLBACK.to_string()),
                    }
                }
                _ => {
                    let list: Vec<String> = found
                        .iter()
                        .map(|p| format!("{} {} {}", p.name, p.size, naira(p.price)))
                        .collect();
                    parts.push(format!("Which one did you mean? {}.", list.join("; ")));
                }
            }
        }
        (Some(c), None) => {
            let items = menu.by_category(c);
            if items.is_empty() {
                parts.push(FALLBACK.to_string());
            } else {
                let list: Vec<String> = items
                    .iter()
                    .map(|p| format!("{} {} {}", p.name, p.size, naira(p.price)))
                    .collect();
                if qty.is_empty() {
                    parts.push(format!("Here's what we have: {}.", list.join("; ")));
                } else {
                    parts.push(format!("Which size would you like? {}.", list.join("; ")));
                }
            }
        }
        (None, _) => {
            if wants_menu {
                parts.push(full_menu(menu, &state.categories));
            } else if intent.topic.as_deref() == Some("price") {
                parts.push(FALLBACK.to_string());
            }
        }
    }

    for t in fact_topics {
        if let Some(a) = state.facts.get(t) {
            parts.push(a.clone());
        }
    }

    if parts.is_empty() {
        parts.push(FALLBACK.to_string());
    }
    Ok(parts.join("\n\n"))
}

#[derive(Deserialize)]
struct ChatReq {
    message: String,
}

#[derive(Serialize)]
struct ChatResp {
    reply: String,
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

async fn chat(State(state): State<Arc<AppState>>, Json(req): Json<ChatReq>) -> Json<ChatResp> {
    let message = req.message.trim();
    let reply = if message.is_empty() {
        "Please type your question.".to_string()
    } else {
        match answer(&state, message).await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[error] {e}");
                "Sorry, I'm having trouble right now. Please message the owner directly."
                    .to_string()
            }
        }
    };
    Json(ChatResp { reply })
}

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    let state = AppState::load()?;
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.first().map(String::as_str) == Some("serve") {
        let app = Router::new()
            .route("/", get(index))
            .route("/chat", post(chat))
            .with_state(Arc::new(state));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
        println!("Listening on http://127.0.0.1:3000");
        axum::serve(listener, app).await?;
        return Ok(());
    }

    let question = args.join(" ");
    if question.is_empty() {
        eprintln!("usage: cargo run -- \"your question\"  |  cargo run -- serve");
        return Ok(());
    }
    println!("{}", answer(&state, &question).await?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_quantity() {
        assert_eq!(quantities("I want 2 parfait 500ml"), vec![2]);
    }

    #[test]
    fn sizes_are_not_quantities() {
        assert!(quantities("1 litre parfait").is_empty());
        assert!(quantities("500ml parfait").is_empty());
    }
}

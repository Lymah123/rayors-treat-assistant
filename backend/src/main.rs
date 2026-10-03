mod llm;
mod menu;

use std::collections::BTreeSet;
use std::collections::HashMap;

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
        "delivery" => &["deliver", "ship", "rider", "location", "fee"],
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

fn infer_topic(question: &str, topics: &[String]) -> Option<String> {
    let q = question.to_lowercase();
    let hits: Vec<&String> = topics
        .iter()
        .filter(|t| topic_keywords(t).iter().any(|k| q.contains(k)))
        .collect();
    if hits.len() == 1 {
        Some(hits[0].clone())
    } else {
        None
    }
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let question = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    if question.is_empty() {
        eprintln!("usage: cargo run -- \"your question\"");
        return Ok(());
    }

    let facts: HashMap<String, String> =
        serde_json::from_str(&std::fs::read_to_string("../data/business.json")?)?;
    let mut topics: Vec<String> = facts.keys().cloned().collect();
    topics.sort();

    let menu = menu::Menu::load("../data/menu.json")?;
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

    let mut intent = llm::extract_intent(&question, &topics, &categories, &sizes).await?;
    println!("[intent] {:?}", intent);

    intent.size = intent.size.take().filter(|s| size_mentioned(&question, s));

    intent.topic = intent
        .topic
        .take()
        .filter(|t| t == "price" || topic_plausible(&question, t));

    if intent.topic.is_none() && intent.category.is_none() {
        intent.topic = infer_topic(&question, &topics);
    }

    println!("[after guard] {:?}", intent);

    if let Some(t) = intent.topic.as_deref() {
        if let Some(answer) = facts.get(t) {
            println!("{answer}");
            return Ok(());
        }
    }

    let reply = match (intent.category.as_deref(), intent.size.as_deref()) {
        (Some(c), Some(s)) => {
            let found = pick(menu.find_all(c, s), &question);
            match found.len() {
                0 => FALLBACK.to_string(),
                1 => {
                    let p = found[0];
                    match quantities(&question).as_slice() {
                        [] | [1] => format!("{} ({}) is {}.", p.name, p.size, naira(p.price)),
                        [q] => format!(
                            "{} x {} ({}) at {} each is {}.",
                            q,
                            p.name,
                            p.size,
                            naira(p.price),
                            naira(p.price * q)
                        ),
                        _ => FALLBACK.to_string(),
                    }
                }
                _ => {
                    let list: Vec<String> = found
                        .iter()
                        .map(|p| format!("{} {} {}", p.name, p.size, naira(p.price)))
                        .collect();
                    format!("Which one did you mean? {}.", list.join("; "))
                }
            }
        }
        (Some(c), None) => {
            let items = menu.by_category(c);
            if items.is_empty() {
                FALLBACK.to_string()
            } else {
                let list: Vec<String> = items
                    .iter()
                    .map(|p| format!("{} {} {}", p.name, p.size, naira(p.price)))
                    .collect();
                format!("Here's what we have: {}.", list.join("; "))
            }
        }
        _ => FALLBACK.to_string(),
    };

    println!("{reply}");
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

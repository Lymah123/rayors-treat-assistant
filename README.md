# Local order assistant for a small food business

A small assistant that answers customers' common questions (prices, delivery, payment, hours) from a business's own price list. It was built for a neighbour's food business, Rayor's Treat, and runs on a local open-weight model (Gemma 3 4B via Ollama). No customer message is sent to a third-party AI service.

Built for the DEV Hacktoberfest Weekend Challenge: Build for a Friend.

## How it works

```
customer message
      |
      v
Gemma (local) -> suggests: topic, product category, size
      |
      v
Rust checks the suggestion (is the size really in the message?
is the topic plausible?) and looks facts up in JSON
      |
      v
reply built from the business data, or "let me confirm with the owner"
```

The model only helps understand the message. Prices, totals and policies come from `data/*.json` and plain Rust code, so the assistant can't invent a price. If the answer isn't in the data, it hands over to the owner.

## Setup

1. Install [Ollama](https://ollama.com) and pull the model: `ollama pull gemma3:4b`
2. Install Rust.
3. Copy the example menu: `cp data/menu.example.json data/menu.json` and edit it for your business. Edit `data/business.json` for your delivery, payment, hours and notice answers.
4. Run the chat page from `backend/`: `cargo run -- serve`, then open http://localhost:3000
5. Or ask a single question: `cargo run -- "how much is a 500ml parfait?"`

## Tests and evaluation

- Unit tests: `cd backend && cargo test`
- Question lists with expected answers: `./eval/run.sh` and `./eval/run-hard.sh`. Add your own questions to `eval/questions.txt`. The expected answers match the example menu.

## Known limits

- It answers each part of a message it recognises, but can't notice a part it doesn't recognise.
- One product per message for totals. Multi-item orders go to the owner.
- Changing prices means editing JSON.
- Not connected to WhatsApp. The webhook and Meta setup are future work, and messages would then pass through Meta's servers.
- Customers won't be told when a question is sent to the owner. Nothing notifies her yet.

## License

MIT
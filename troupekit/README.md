# Troupekit

Troupekit is a Rust framework for building LLM agents from persona TOML files: providers, tools, memory, RAG and durable graph workflows.

| Crate | Status | What |
|---|---|---|
| `troupekit-core` | available | Messages, secrets, errors, events, ids, clock, storage traits and in-memory stores |
| `troupekit-providers` | available | Anthropic, OpenAI, Gemini and OpenAI-compatible wire protocols, provider catalog with prices, injected transports, retry, fallback and cost-meter middleware |
| `troupekit-tools` | available | Tool trait, validating registry, typed tools, SSRF-guarded web fetch, web search over Brave and SearXNG, declarative HTTP tools |
| `troupekit-context` | available | Memory scopes, profile facts, chunking, embeddings, hybrid retrieval and context blocks |
| `troupekit-agent` | planned | Persona TOML agents |
| `troupekit-graph` | planned | Durable, checkpointed graph workflows |
| `troupekit-plugins` | planned | Plugin loading |
| `troupekit-host` | planned | Hosting runs and streaming events |

## License

Licensed under either of MIT or Apache-2.0, at your option.

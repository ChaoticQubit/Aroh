/** One vendor of the offline provider list: id, display name, wire protocol and its models. */
export type CatalogEntry = { id: string; name: string; protocol: string; models: string[]; selfHosted: boolean };

/** Model providers offered in Settings until the engine serves its own catalog. */
export const catalog: CatalogEntry[] = [
	{ id: 'anthropic', name: 'Anthropic', protocol: 'anthropic', models: ['claude-opus-5-5', 'claude-sonnet-5-5', 'claude-haiku-5-5'], selfHosted: false },
	{ id: 'openai', name: 'OpenAI', protocol: 'openai', models: ['gpt-5', 'gpt-5-mini'], selfHosted: false },
	{ id: 'gemini', name: 'Google Gemini', protocol: 'gemini', models: ['gemini-2.5-pro', 'gemini-2.5-flash'], selfHosted: false },
	{ id: 'openrouter', name: 'OpenRouter', protocol: 'openai-compatible', models: ['openrouter/auto'], selfHosted: false },
	{ id: 'groq', name: 'Groq', protocol: 'openai-compatible', models: ['llama-3.3-70b-versatile'], selfHosted: false },
	{ id: 'ollama', name: 'Ollama (self-hosted)', protocol: 'ollama', models: ['qwen3:8b', 'llama3.1:8b'], selfHosted: true }
];

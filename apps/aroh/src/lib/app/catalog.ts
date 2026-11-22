/** One vendor of the provider list: id, display name, wire protocol and its models. */
export type CatalogEntry = { id: string; name: string; protocol: string; models: string[]; selfHosted: boolean };

/**
 * Model providers offered in Settings, with the vendor and model ids of the engine catalog
 * (`troupekit/catalog/providers.toml`), so a saved provider row resolves on the server.
 * Only Anthropic prices are verified there; other vendors pause runs at `unverified_price`.
 */
export const catalog: CatalogEntry[] = [
	{ id: 'anthropic', name: 'Anthropic', protocol: 'anthropic', models: ['claude-sonnet-5-5', 'claude-opus-5-5', 'claude-haiku-5-5'], selfHosted: false },
	{ id: 'openai', name: 'OpenAI', protocol: 'openai', models: ['gpt-6.2-sol', 'gpt-6.1-terra', 'gpt-6-luna'], selfHosted: false },
	{ id: 'google', name: 'Google Gemini', protocol: 'gemini', models: ['gemini-3.8-flash', 'gemini-3.5-flash-lite', 'gemini-3.1-pro-preview'], selfHosted: false },
	{ id: 'openrouter', name: 'OpenRouter', protocol: 'openai_compatible', models: ['anthropic/claude-opus-5.5', 'anthropic/claude-haiku-5.5'], selfHosted: false },
	{ id: 'groq', name: 'Groq', protocol: 'openai_compatible', models: ['llama-3.3-70b-versatile', 'openai/gpt-oss-120b'], selfHosted: false },
	{ id: 'deepseek', name: 'DeepSeek', protocol: 'openai_compatible', models: ['deepseek-v4-pro', 'deepseek-v4-flash'], selfHosted: false },
	{ id: 'mistral', name: 'Mistral', protocol: 'openai_compatible', models: ['mistral-large-latest', 'mistral-small-latest'], selfHosted: false },
	{ id: 'ollama', name: 'Ollama (local)', protocol: 'openai_compatible', models: ['qwen3:8b', 'llama3.1:8b'], selfHosted: true }
];

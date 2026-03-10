export const rigProviderTypeOptions = [
  "OpenAI",
  "Anthropic",
  "Gemini",
  "DeepSeek",
  "Groq",
  "Cohere",
  "xAI",
  "Mistral",
  "Moonshot",
  "Hyperbolic",
  "Mira",
  "OpenRouter",
  "Perplexity",
  "Together",
  "HuggingFace",
  "Ollama",
  "Azure",
  "Galadriel",
  "VoyageAI",
] as const;

export const runtimeSupportedRigProviderTypes = new Set<string>([
  "OpenAI",
  "Anthropic",
  "Gemini",
  "DeepSeek",
  "Groq",
  "Cohere",
  "xAI",
  "Mistral",
  "Moonshot",
  "Hyperbolic",
  "Mira",
  "OpenRouter",
  "Perplexity",
  "Together",
  "HuggingFace",
  "Ollama",
  "Azure",
  "Galadriel",
]);

export const providerTypeDefaults: Record<
  string,
  { baseUrl: string; defaultModel: string; models: string[] }
> = {
  OpenAI: {
    baseUrl: "https://api.openai.com/v1",
    defaultModel: "gpt-4o-mini",
    models: ["gpt-4o-mini", "gpt-4o", "o3-mini"],
  },
  Anthropic: {
    baseUrl: "https://api.anthropic.com",
    defaultModel: "claude-3-5-sonnet-20241022",
    models: ["claude-3-5-sonnet-20241022", "claude-3-5-haiku-20241022"],
  },
  Gemini: {
    baseUrl: "https://generativelanguage.googleapis.com/v1beta",
    defaultModel: "gemini-2.0-flash",
    models: ["gemini-2.0-flash", "gemini-1.5-pro"],
  },
  DeepSeek: {
    baseUrl: "https://api.deepseek.com",
    defaultModel: "deepseek-chat",
    models: ["deepseek-chat", "deepseek-reasoner"],
  },
  Groq: {
    baseUrl: "https://api.groq.com/openai/v1",
    defaultModel: "llama-3.3-70b-versatile",
    models: ["llama-3.3-70b-versatile", "llama-3.1-8b-instant"],
  },
  Cohere: {
    baseUrl: "https://api.cohere.ai",
    defaultModel: "command-r-plus",
    models: ["command-r-plus", "command-r"],
  },
  xAI: {
    baseUrl: "https://api.x.ai/v1",
    defaultModel: "grok-3-mini",
    models: ["grok-3-mini", "grok-3"],
  },
  Mistral: {
    baseUrl: "https://api.mistral.ai/v1",
    defaultModel: "mistral-large-latest",
    models: ["mistral-large-latest", "ministral-8b-latest"],
  },
  Moonshot: {
    baseUrl: "https://api.moonshot.ai/v1",
    defaultModel: "kimi-k2-0905-preview",
    models: ["kimi-k2-0905-preview", "moonshot-v1-8k"],
  },
  Hyperbolic: {
    baseUrl: "https://api.hyperbolic.xyz/v1",
    defaultModel: "meta-llama/Meta-Llama-3-70B-Instruct",
    models: ["meta-llama/Meta-Llama-3-70B-Instruct"],
  },
  Mira: {
    baseUrl: "https://api.mira.network/v1",
    defaultModel: "mira-chat",
    models: ["mira-chat"],
  },
  OpenRouter: {
    baseUrl: "https://openrouter.ai/api/v1",
    defaultModel: "openai/gpt-4o-mini",
    models: ["openai/gpt-4o-mini", "anthropic/claude-3.5-sonnet"],
  },
  Perplexity: {
    baseUrl: "https://api.perplexity.ai",
    defaultModel: "llama-3.1-sonar-small-128k-online",
    models: ["llama-3.1-sonar-small-128k-online"],
  },
  Together: {
    baseUrl: "https://api.together.xyz/v1",
    defaultModel: "meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo",
    models: ["meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo"],
  },
  HuggingFace: {
    baseUrl: "https://router.huggingface.co/v1",
    defaultModel: "meta-llama/Meta-Llama-3-8B-Instruct",
    models: ["meta-llama/Meta-Llama-3-8B-Instruct"],
  },
  Ollama: {
    baseUrl: "http://localhost:11434",
    defaultModel: "qwen2.5:14b",
    models: ["qwen2.5:14b", "llama3.1:8b"],
  },
  Azure: {
    baseUrl: "https://<your-resource>.openai.azure.com",
    defaultModel: "gpt-4o-mini",
    models: ["gpt-4o-mini"],
  },
  Galadriel: {
    baseUrl: "https://api.galadriel.com/v1/verified",
    defaultModel: "gpt-4o",
    models: ["gpt-4o"],
  },
  VoyageAI: {
    baseUrl: "https://api.voyageai.com/v1",
    defaultModel: "voyage-3-large",
    models: ["voyage-3-large"],
  },
};

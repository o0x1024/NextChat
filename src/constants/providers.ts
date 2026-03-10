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
    defaultModel: "",
    models: [],
  },
  Anthropic: {
    baseUrl: "https://api.anthropic.com",
    defaultModel: "",
    models: [],
  },
  Gemini: {
    baseUrl: "https://generativelanguage.googleapis.com/v1beta",
    defaultModel: "",
    models: [],
  },
  DeepSeek: {
    baseUrl: "https://api.deepseek.com",
    defaultModel: "",
    models: [],
  },
  Groq: {
    baseUrl: "https://api.groq.com/openai/v1",
    defaultModel: "",
    models: [],
  },
  Cohere: {
    baseUrl: "https://api.cohere.ai",
    defaultModel: "",
    models: [],
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
    defaultModel: "",
    models: [],
  },
  Perplexity: {
    baseUrl: "https://api.perplexity.ai",
    defaultModel: "",
    models: [],
  },
  Together: {
    baseUrl: "https://api.together.xyz/v1",
    defaultModel: "",
    models: [],
  },
  HuggingFace: {
    baseUrl: "https://router.huggingface.co/v1",
    defaultModel: "",
    models: [],
  },
  Ollama: {
    baseUrl: "http://localhost:11434",
    defaultModel: "",
    models: [],
  },
  Azure: {
    baseUrl: "https://<your-resource>.openai.azure.com",
    defaultModel: "",
    models: [],
  },
  Galadriel: {
    baseUrl: "https://api.galadriel.com/v1/verified",
    defaultModel: "",
    models: [],
  },
  VoyageAI: {
    baseUrl: "https://api.voyageai.com/v1",
    defaultModel: "",
    models: [],
  },
};

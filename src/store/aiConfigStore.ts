import { create } from "zustand";
import { persist } from "zustand/middleware";

export interface AIProviderConfig {
    id: string;
    name: string;
    icon: string;
    enabled: boolean;
    rigProviderType: string;
    apiKey: string;
    baseUrl: string;
    models: string[];
    defaultModel: string;
    temperature: number;
    maxTokens: number;
    outputTokenLimit: number;
    maxDialogRounds: number;
}

export interface AIGlobalConfig {
    defaultLLMProvider: string;
    defaultLLMModel: string;
    defaultVLMProvider: string;
    defaultVLMModel: string;
}

export const defaultProviders: AIProviderConfig[] = [
    {
        id: "openai",
        name: "OpenAI",
        icon: "fab fa-openai",
        enabled: true,
        rigProviderType: "OpenAI",
        apiKey: "",
        baseUrl: "https://api.openai.com/v1",
        models: ["gpt-4o", "gpt-4o-mini", "gpt-4-turbo", "gpt-3.5-turbo", "o1", "o1-mini", "o3-mini"],
        defaultModel: "gpt-4o",
        temperature: 0.7,
        maxTokens: 2000,
        outputTokenLimit: 16384,
        maxDialogRounds: 540,
    },
    {
        id: "anthropic",
        name: "Anthropic",
        icon: "fas fa-comment-dots",
        enabled: false,
        rigProviderType: "Anthropic",
        apiKey: "",
        baseUrl: "https://api.anthropic.com",
        models: ["claude-sonnet-4-20250514", "claude-3-5-haiku-20241022", "claude-3-opus-20240229"],
        defaultModel: "claude-sonnet-4-20250514",
        temperature: 0.7,
        maxTokens: 2000,
        outputTokenLimit: 16384,
        maxDialogRounds: 540,
    },
    {
        id: "deepseek",
        name: "DeepSeek",
        icon: "fas fa-water",
        enabled: false,
        rigProviderType: "DeepSeek",
        apiKey: "",
        baseUrl: "https://api.deepseek.com",
        models: ["deepseek-chat", "deepseek-coder", "deepseek-reasoner"],
        defaultModel: "deepseek-chat",
        temperature: 0.7,
        maxTokens: 2000,
        outputTokenLimit: 16384,
        maxDialogRounds: 540,
    },
    {
        id: "gemini",
        name: "Gemini",
        icon: "fab fa-google",
        enabled: false,
        rigProviderType: "Gemini",
        apiKey: "",
        baseUrl: "https://generativelanguage.googleapis.com/v1beta",
        models: ["gemini-2.5-flash", "gemini-2.5-pro", "gemini-2.0-flash", "gemini-1.5-pro"],
        defaultModel: "gemini-2.5-flash",
        temperature: 0.7,
        maxTokens: 2000,
        outputTokenLimit: 16384,
        maxDialogRounds: 540,
    },
    {
        id: "groq",
        name: "Groq",
        icon: "fas fa-bolt",
        enabled: false,
        rigProviderType: "Groq",
        apiKey: "",
        baseUrl: "https://api.groq.com/openai/v1",
        models: ["llama-3.3-70b-versatile", "llama-3.1-8b-instant", "mixtral-8x7b-32768"],
        defaultModel: "llama-3.3-70b-versatile",
        temperature: 0.7,
        maxTokens: 2000,
        outputTokenLimit: 16384,
        maxDialogRounds: 540,
    },
    {
        id: "cohere",
        name: "Cohere",
        icon: "fas fa-circle",
        enabled: false,
        rigProviderType: "Cohere",
        apiKey: "",
        baseUrl: "https://api.cohere.ai/v1",
        models: ["command-r-plus", "command-r", "command-light"],
        defaultModel: "command-r-plus",
        temperature: 0.7,
        maxTokens: 2000,
        outputTokenLimit: 16384,
        maxDialogRounds: 540,
    },
    {
        id: "aliyun",
        name: "Aliyun",
        icon: "fas fa-cloud",
        enabled: false,
        rigProviderType: "OpenAI",
        apiKey: "",
        baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
        models: ["qwen-max", "qwen-plus", "qwen-turbo", "qwen-long"],
        defaultModel: "qwen-max",
        temperature: 0.7,
        maxTokens: 2000,
        outputTokenLimit: 16384,
        maxDialogRounds: 540,
    },
    {
        id: "hyperbolic",
        name: "Hyperbolic",
        icon: "fas fa-infinity",
        enabled: false,
        rigProviderType: "OpenAI",
        apiKey: "",
        baseUrl: "https://api.hyperbolic.xyz/v1",
        models: ["meta-llama/Meta-Llama-3-70B-Instruct"],
        defaultModel: "meta-llama/Meta-Llama-3-70B-Instruct",
        temperature: 0.7,
        maxTokens: 2000,
        outputTokenLimit: 16384,
        maxDialogRounds: 540,
    },
    {
        id: "lmstudio",
        name: "LM Studio",
        icon: "fas fa-home",
        enabled: false,
        rigProviderType: "OpenAI",
        apiKey: "lm-studio",
        baseUrl: "http://localhost:1234/v1",
        models: ["local-model"],
        defaultModel: "local-model",
        temperature: 0.7,
        maxTokens: 2000,
        outputTokenLimit: 16384,
        maxDialogRounds: 540,
    },
];

interface AIConfigState {
    providers: AIProviderConfig[];
    globalConfig: AIGlobalConfig;
    selectedProviderId: string;
    setSelectedProviderId: (id: string) => void;
    updateProvider: (id: string, updates: Partial<AIProviderConfig>) => void;
    updateGlobalConfig: (updates: Partial<AIGlobalConfig>) => void;
    resetProvider: (id: string) => void;
    getActiveProvider: () => AIProviderConfig | undefined;
}

export const useAIConfigStore = create<AIConfigState>()(
    persist(
        (set, get) => ({
            providers: defaultProviders,
            globalConfig: {
                defaultLLMProvider: "openai",
                defaultLLMModel: "gpt-4o",
                defaultVLMProvider: "gemini",
                defaultVLMModel: "gemini-2.5-flash",
            },
            selectedProviderId: "openai",

            setSelectedProviderId(id) {
                set({ selectedProviderId: id });
            },

            updateProvider(id, updates) {
                set((state) => ({
                    providers: state.providers.map((p) =>
                        p.id === id ? { ...p, ...updates } : p
                    ),
                }));
            },

            updateGlobalConfig(updates) {
                set((state) => ({
                    globalConfig: { ...state.globalConfig, ...updates },
                }));
            },

            resetProvider(id) {
                const original = defaultProviders.find((p) => p.id === id);
                if (original) {
                    set((state) => ({
                        providers: state.providers.map((p) =>
                            p.id === id ? { ...original } : p
                        ),
                    }));
                }
            },

            getActiveProvider() {
                const state = get();
                return state.providers.find((p) => p.id === state.selectedProviderId);
            },
        }),
        {
            name: "nextchat-ai-config",
        }
    )
);

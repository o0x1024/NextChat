import type { AIProviderConfig, AgentProfile, CreateAgentInput, SystemSettings } from "../../types";
import { runtimeSupportedRigProviderTypes } from "../../constants/providers";
import { emptyMemoryPolicy, emptyPermissionPolicy } from "./agentPermissions";

export type ProviderReason = "disabled" | "missingApiKey" | "unsupported" | "noModels" | null;

export type ProviderAvailability = {
  provider: AIProviderConfig;
  available: boolean;
  reason: ProviderReason;
};

export function buildProviderAvailability(provider: AIProviderConfig): ProviderAvailability {
  if (!provider.enabled) {
    return { provider, available: false, reason: "disabled" };
  }
  if (!runtimeSupportedRigProviderTypes.has(provider.rigProviderType)) {
    return { provider, available: false, reason: "unsupported" };
  }
  if (provider.models.length === 0) {
    return { provider, available: false, reason: "noModels" };
  }
  if (requiresApiKey(provider) && provider.apiKey.trim().length === 0) {
    return { provider, available: false, reason: "missingApiKey" };
  }
  return { provider, available: true, reason: null };
}

export const emptyForm: CreateAgentInput = {
  name: "",
  avatar: "AX",
  role: "",
  objective: "",
  provider: "openai",
  model: "gpt-4o",
  temperature: 0.7,
  skillIds: [],
  toolIds: [],
  maxParallelRuns: 2,
  canSpawnSubtasks: true,
  memoryPolicy: emptyMemoryPolicy,
  permissionPolicy: emptyPermissionPolicy,
};

export function requiresApiKey(provider: AIProviderConfig): boolean {
  return provider.rigProviderType !== "Ollama";
}

export function isProviderAvailable(provider: AIProviderConfig): boolean {
  return (
    provider.enabled &&
    runtimeSupportedRigProviderTypes.has(provider.rigProviderType) &&
    provider.models.length > 0 &&
    (!requiresApiKey(provider) || provider.apiKey.trim().length > 0)
  );
}

export function normalizeProviderValue(value: string | null | undefined): string {
  return typeof value === "string" ? value.trim().toLowerCase() : "";
}

export function findProviderMatch(
  providers: AIProviderConfig[],
  rawProvider: string,
): AIProviderConfig | undefined {
  const normalizedProvider = normalizeProviderValue(rawProvider);
  if (!normalizedProvider) {
    return undefined;
  }

  return providers.find((provider) => {
    const candidates = [provider.id, provider.name, provider.rigProviderType];
    return candidates.some((candidate) => normalizeProviderValue(candidate) === normalizedProvider);
  });
}

export function resolveProviderModel(
  provider: AIProviderConfig | undefined,
  rawModel: string | null | undefined,
  globalDefaultModel: string,
): string {
  const model = typeof rawModel === "string" ? rawModel.trim() : "";
  const normalizedGlobalDefaultModel = globalDefaultModel.trim();

  if (!provider) {
    return model || normalizedGlobalDefaultModel;
  }
  if (provider.models.includes(model)) {
    return model;
  }
  if (!model || normalizeProviderValue(model) === "simulation") {
    if (normalizedGlobalDefaultModel && provider.models.includes(normalizedGlobalDefaultModel)) {
      return normalizedGlobalDefaultModel;
    }
    return provider.defaultModel || provider.models[0] || normalizedGlobalDefaultModel || "";
  }

  return model;
}

export function normalizeModelForm(
  settings: SystemSettings,
  overrides: Partial<Pick<CreateAgentInput, "provider" | "model" | "temperature">> = {},
): Pick<CreateAgentInput, "provider" | "model" | "temperature"> {
  const globalDefaultProvider = settings.globalConfig?.defaultLLMProvider ?? "";
  const globalDefaultModel = settings.globalConfig?.defaultLLMModel ?? "";
  const availableProviders = settings.providers.filter(isProviderAvailable);
  const preferredProvider =
    findProviderMatch(settings.providers, overrides.provider ?? globalDefaultProvider) ??
    settings.providers.find(
      (provider) =>
        normalizeProviderValue(provider.id) === normalizeProviderValue(globalDefaultProvider),
    );

  const selectedProvider = [preferredProvider, ...availableProviders].find(
    (provider): provider is AIProviderConfig => Boolean(provider && isProviderAvailable(provider)),
  );

  const fallbackProvider = selectedProvider ?? availableProviders[0];
  const provider = fallbackProvider?.id ?? "";
  const model = resolveProviderModel(fallbackProvider, overrides.model ?? "", globalDefaultModel);
  const temperature = overrides.temperature ?? fallbackProvider?.temperature ?? emptyForm.temperature;

  return { provider, model, temperature };
}

export function buildCreateForm(settings: SystemSettings): CreateAgentInput {
  return {
    ...emptyForm,
    ...normalizeModelForm(settings),
  };
}

export function normalizeGeneratedAgentInput(
  settings: SystemSettings,
  generated: CreateAgentInput,
): CreateAgentInput {
  return {
    ...generated,
    ...normalizeModelForm(settings, {
      provider: generated.provider,
      model: generated.model,
      temperature: generated.temperature,
    }),
  };
}

export function isBuiltinGroupOwner(agent: AgentProfile): boolean {
  const role = agent.role.trim().toLowerCase();
  return role === "group owner" || agent.name === "群主";
}

export function serializeAgentConfig(agent: AgentProfile) {
  return {
    sourceAgentId: agent.id,
    name: agent.name,
    avatar: agent.avatar,
    role: agent.role,
    objective: agent.objective,
    provider: agent.modelPolicy.provider,
    model: agent.modelPolicy.model,
    temperature: agent.modelPolicy.temperature,
    skillIds: agent.skillIds,
    toolIds: agent.toolIds,
    maxParallelRuns: agent.maxParallelRuns,
    canSpawnSubtasks: agent.canSpawnSubtasks,
    memoryPolicy: agent.memoryPolicy,
    permissionPolicy: agent.permissionPolicy,
  };
}

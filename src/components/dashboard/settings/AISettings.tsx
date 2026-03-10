import { type ChangeEvent, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { refreshProviderModels, testProviderConnection } from "../../../lib/tauri";
import { useAppStore } from "../../../store/appStore";
import type { AIProviderConfig } from "../../../types";
import { providerTypeDefaults, rigProviderTypeOptions } from "../../../constants/providers";

type NewProviderForm = {
  id: string;
  name: string;
  rigProviderType: string;
  apiKey: string;
  baseUrl: string;
  defaultModel: string;
  icon: string;
};

const defaultNewProviderType = "OpenAI";

const emptyNewProviderForm: NewProviderForm = {
  id: "",
  name: "",
  rigProviderType: defaultNewProviderType,
  apiKey: "",
  baseUrl: providerTypeDefaults[defaultNewProviderType]?.baseUrl ?? "",
  defaultModel: providerTypeDefaults[defaultNewProviderType]?.defaultModel ?? "",
  icon: "fas fa-plug",
};

function ProviderListItem({
  provider,
  isSelected,
  onClick,
}: {
  provider: AIProviderConfig;
  isSelected: boolean;
  onClick: () => void;
}) {
  return (
    <button
      className={`flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-left transition-all duration-200 ${
        isSelected
          ? "border border-primary/20 bg-primary/10 text-primary"
          : "hover:bg-base-300/60"
      }`}
      onClick={onClick}
      type="button"
    >
      <i className={`${provider.icon} w-5 text-center`} />
      <span className="text-sm font-medium">{provider.name}</span>
    </button>
  );
}

function normalizeProviderId(input: string) {
  return input
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9-_]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

function providerModules(rigProviderType: string): string[] {
  if (rigProviderType === "VoyageAI") {
    return ["embeddings"];
  }
  if (rigProviderType === "Ollama") {
    return ["chat", "streaming", "local-runtime"];
  }
  if (rigProviderType === "Gemini") {
    return ["chat", "streaming", "tools", "multimodal"];
  }
  return ["chat", "streaming", "tools"];
}

function providerRequiresApiKey(rigProviderType: string): boolean {
  return rigProviderType !== "Ollama";
}

export function AISettings() {
  const { t } = useTranslation();
  const {
    settings,
    selectedSettingsProviderId: selectedProviderId,
    setSelectedSettingsProviderId: setSelectedProviderId,
    refresh,
    updateSettings,
  } = useAppStore();

  const { providers } = settings;
  const activeProvider = providers.find((p) => p.id === selectedProviderId);
  const rawMaxContextLength = activeProvider?.maxContextLength;
  const activeProviderMaxContextLength: number =
    typeof rawMaxContextLength === "number" && Number.isFinite(rawMaxContextLength)
      ? rawMaxContextLength
      : 128000;
  const activeProviderCustomHeaders = activeProvider?.customHeaders ?? "{}";

  const [showApiKey, setShowApiKey] = useState(false);
  const [testStatus, setTestStatus] = useState<"idle" | "testing" | "success" | "error">("idle");
  const [refreshStatus, setRefreshStatus] = useState<"idle" | "refreshing" | "success" | "error">(
    "idle",
  );
  const [refreshMessage, setRefreshMessage] = useState("");
  const [formMessage, setFormMessage] = useState("");
  const [newProviderForm, setNewProviderForm] = useState<NewProviderForm>(emptyNewProviderForm);

  const updateProvider = (id: string, updates: Partial<AIProviderConfig>) => {
    const nextProviders = providers.map((p) => (p.id === id ? { ...p, ...updates } : p));
    void updateSettings({ ...settings, providers: nextProviders });
  };

  const sortedModels = useMemo(() => activeProvider?.models ?? [], [activeProvider]);

  async function handleTestConnection() {
    if (!activeProvider) return;
    setTestStatus("testing");
    try {
      await testProviderConnection(activeProvider);
      setTestStatus("success");
    } catch (error) {
      console.error("Connection test failed:", error);
      setTestStatus("error");
    } finally {
      setTimeout(() => setTestStatus("idle"), 3000);
    }
  }

  async function handleRefreshModels() {
    if (!activeProvider) return;
    setRefreshStatus("refreshing");
    setRefreshMessage("");
    try {
      const updatedProvider = await refreshProviderModels(activeProvider);
      await refresh();
      setRefreshStatus("success");
      setRefreshMessage(t("modelsRefreshed", { count: updatedProvider.models.length }));
    } catch (error) {
      console.error("Model refresh failed:", error);
      setRefreshStatus("error");
      setRefreshMessage(error instanceof Error ? error.message : t("modelRefreshFailed"));
    } finally {
      setTimeout(() => {
        setRefreshStatus("idle");
        setRefreshMessage("");
      }, 3000);
    }
  }

  async function handleAddProvider() {
    const providerId = normalizeProviderId(newProviderForm.id || newProviderForm.name);
    if (!providerId) {
      setFormMessage(t("providerIdRequired"));
      return;
    }
    if (providers.some((provider) => provider.id === providerId)) {
      setFormMessage(t("providerIdExists"));
      return;
    }

    if (
      providerRequiresApiKey(newProviderForm.rigProviderType) &&
      !newProviderForm.apiKey.trim()
    ) {
      setFormMessage(t("providerApiKeyRequired"));
      return;
    }

    const defaults = providerTypeDefaults[newProviderForm.rigProviderType] ?? providerTypeDefaults.OpenAI;
    const defaultModel = newProviderForm.defaultModel.trim() || defaults.defaultModel;
    const provider: AIProviderConfig = {
      id: providerId,
      name: newProviderForm.name.trim() || providerId,
      icon: newProviderForm.icon.trim() || "fas fa-plug",
      enabled: true,
      rigProviderType: newProviderForm.rigProviderType,
      apiKey: newProviderForm.apiKey.trim(),
      baseUrl: (newProviderForm.baseUrl.trim() || defaults.baseUrl).trim(),
      models: [defaultModel, ...defaults.models].filter(
        (model, index, arr) => model.trim().length > 0 && arr.indexOf(model) === index,
      ),
      defaultModel,
      maxContextLength: 128000,
      customHeaders: "{}",
      temperature: 0.7,
      maxTokens: 2000,
      outputTokenLimit: 16384,
      maxDialogRounds: 540,
    };

    setFormMessage("");
    await updateSettings({ ...settings, providers: [...providers, provider] });
    setSelectedProviderId(provider.id);
    setNewProviderForm({
      ...emptyNewProviderForm,
      rigProviderType: provider.rigProviderType,
      baseUrl: provider.baseUrl,
      defaultModel: provider.defaultModel,
    });
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-bold">{t("aiConfiguration")}</h2>
        <span className="badge badge-ghost text-xs">{t("graphicalMode")}</span>
      </div>

      <div className="card card-border bg-base-100 shadow-sm">
        <div className="card-body gap-4 p-4">
          <div className="flex items-center gap-2">
            <i className="fas fa-circle-plus text-primary" />
            <h3 className="text-base font-bold">{t("addCustomProvider")}</h3>
          </div>
          <div className="grid grid-cols-1 gap-3 xl:grid-cols-2">
            <div>
              <label className="label">
                <span className="label-text text-xs">{t("providerId")}</span>
              </label>
              <input
                className="input input-bordered input-sm w-full"
                value={newProviderForm.id}
                placeholder="my-custom-provider"
                onChange={(event) =>
                  setNewProviderForm((current) => ({ ...current, id: event.target.value }))
                }
              />
            </div>
            <div>
              <label className="label">
                <span className="label-text text-xs">{t("displayName")}</span>
              </label>
              <input
                className="input input-bordered input-sm w-full"
                value={newProviderForm.name}
                placeholder="My Custom Provider"
                onChange={(event) =>
                  setNewProviderForm((current) => ({ ...current, name: event.target.value }))
                }
              />
            </div>
            <div>
              <label className="label">
                <span className="label-text text-xs">{t("rigProviderType")}</span>
              </label>
              <select
                className="select select-bordered select-sm w-full"
                value={newProviderForm.rigProviderType}
                onChange={(event) => {
                  const nextType = event.target.value;
                  const defaults = providerTypeDefaults[nextType] ?? providerTypeDefaults.OpenAI;
                  setNewProviderForm((current) => ({
                    ...current,
                    rigProviderType: nextType,
                    baseUrl: defaults.baseUrl,
                    defaultModel: defaults.defaultModel,
                  }));
                }}
              >
                {rigProviderTypeOptions.map((type) => (
                  <option key={type} value={type}>
                    {type}
                  </option>
                ))}
              </select>
              <div className="mt-1 text-xs text-base-content/50">{t("rigProviderTypeHint")}</div>
            </div>
            <div>
              <label className="label">
                <span className="label-text text-xs">{t("apiBaseUrl")}</span>
              </label>
              <input
                className="input input-bordered input-sm w-full"
                value={newProviderForm.baseUrl}
                onChange={(event) =>
                  setNewProviderForm((current) => ({ ...current, baseUrl: event.target.value }))
                }
              />
            </div>
            <div>
              <label className="label">
                <span className="label-text text-xs">{t("apiKey")}</span>
              </label>
              <input
                className="input input-bordered input-sm w-full"
                value={newProviderForm.apiKey}
                placeholder="sk-..."
                onChange={(event) =>
                  setNewProviderForm((current) => ({ ...current, apiKey: event.target.value }))
                }
              />
            </div>
            <div>
              <label className="label">
                <span className="label-text text-xs">{t("defaultModel")}</span>
              </label>
              <input
                className="input input-bordered input-sm w-full"
                value={newProviderForm.defaultModel}
                onChange={(event) =>
                  setNewProviderForm((current) => ({ ...current, defaultModel: event.target.value }))
                }
              />
            </div>
          </div>
          {formMessage ? <div className="alert alert-error py-2 text-sm">{formMessage}</div> : null}
          <div className="flex justify-end">
            <button className="btn btn-primary btn-sm" type="button" onClick={handleAddProvider}>
              <i className="fas fa-plus" />
              {t("addProvider")}
            </button>
          </div>
        </div>
      </div>

      <div className="grid grid-cols-1 gap-6 xl:grid-cols-[220px_1fr_1fr]">
        <div className="space-y-2">
          <div className="px-1 text-sm font-semibold">{t("aiProviders")}</div>
          <div className="space-y-1">
            {providers.map((provider) => (
              <ProviderListItem
                key={provider.id}
                provider={provider}
                isSelected={selectedProviderId === provider.id}
                onClick={() => setSelectedProviderId(provider.id)}
              />
            ))}
          </div>
        </div>

        {activeProvider ? (
          <div className="space-y-4">
            <div className="px-1 text-sm font-semibold">{t("basicConfiguration")}</div>
            <div className="card card-border bg-base-100 shadow-sm">
              <div className="card-body gap-4 p-4">
                <div className="flex items-center justify-between">
                  <span className="text-sm">{t("enableProvider", { name: activeProvider.name })}</span>
                  <input
                    type="checkbox"
                    className="toggle toggle-primary toggle-sm"
                    checked={activeProvider.enabled}
                    onChange={(event: ChangeEvent<HTMLInputElement>) =>
                      updateProvider(activeProvider.id, { enabled: event.target.checked })
                    }
                  />
                </div>
                <div className="divider my-0" />
                <div>
                  <label className="label">
                    <span className="label-text text-xs">
                      {t("rigProviderType")}
                      <span className="badge badge-primary badge-xs ml-2">{t("rigApiFormat")}</span>
                    </span>
                  </label>
                  <select
                    className="select select-bordered select-sm w-full"
                    value={activeProvider.rigProviderType}
                    onChange={(event: ChangeEvent<HTMLSelectElement>) =>
                      updateProvider(activeProvider.id, { rigProviderType: event.target.value })
                    }
                  >
                    {rigProviderTypeOptions.map((type) => (
                      <option key={type} value={type}>
                        {type}
                      </option>
                    ))}
                  </select>
                </div>
                <div>
                  <label className="label">
                    <span className="label-text text-xs">{t("apiKey")}</span>
                  </label>
                  <div className="join w-full">
                    <input
                      className="input input-bordered input-sm join-item flex-1"
                      type={showApiKey ? "text" : "password"}
                      placeholder="sk-..."
                      value={activeProvider.apiKey}
                      onChange={(event: ChangeEvent<HTMLInputElement>) =>
                        updateProvider(activeProvider.id, { apiKey: event.target.value })
                      }
                    />
                    <button
                      className="btn btn-ghost btn-sm join-item"
                      type="button"
                      onClick={() => setShowApiKey(!showApiKey)}
                    >
                      <i className={showApiKey ? "fas fa-eye-slash" : "fas fa-eye"} />
                    </button>
                    <button
                      className={`btn btn-sm join-item ${
                        testStatus === "testing"
                          ? "btn-disabled"
                          : testStatus === "success"
                            ? "btn-success"
                            : testStatus === "error"
                              ? "btn-error"
                              : "btn-ghost"
                      }`}
                      type="button"
                      onClick={handleTestConnection}
                    >
                      {testStatus === "testing" ? (
                        <span className="loading loading-spinner loading-xs" />
                      ) : testStatus === "success" ? (
                        "✓"
                      ) : testStatus === "error" ? (
                        "✕"
                      ) : null}
                      {t("testConnection")}
                    </button>
                  </div>
                </div>
                <div>
                  <label className="label">
                    <span className="label-text text-xs">{t("defaultModel")}</span>
                    <button
                      className={`btn btn-ghost btn-xs ${refreshStatus === "refreshing" ? "btn-disabled" : ""}`}
                      type="button"
                      onClick={handleRefreshModels}
                    >
                      {refreshStatus === "refreshing" ? (
                        <span className="loading loading-spinner loading-xs" />
                      ) : (
                        <i className="fas fa-rotate" />
                      )}
                      {t("refreshModels")}
                    </button>
                  </label>
                  <select
                    className="select select-bordered select-sm w-full"
                    value={activeProvider.defaultModel}
                    onChange={(event: ChangeEvent<HTMLSelectElement>) =>
                      updateProvider(activeProvider.id, { defaultModel: event.target.value })
                    }
                  >
                    {activeProvider.models.map((model) => (
                      <option key={model} value={model}>
                        {model}
                      </option>
                    ))}
                  </select>
                  <input
                    className="input input-bordered input-sm mt-2 w-full"
                    value={activeProvider.defaultModel}
                    onChange={(event: ChangeEvent<HTMLInputElement>) =>
                      updateProvider(activeProvider.id, { defaultModel: event.target.value })
                    }
                    placeholder={t("defaultModel")}
                  />
                  {refreshMessage ? (
                    <div
                      className={`mt-2 text-xs ${refreshStatus === "error" ? "text-error" : "text-base-content/60"}`}
                    >
                      {refreshMessage}
                    </div>
                  ) : null}
                </div>
                <div>
                  <label className="label">
                    <span className="label-text text-xs">{t("apiBaseUrl")}</span>
                  </label>
                  <input
                    className="input input-bordered input-sm w-full"
                    value={activeProvider.baseUrl}
                    onChange={(event: ChangeEvent<HTMLInputElement>) =>
                      updateProvider(activeProvider.id, { baseUrl: event.target.value })
                    }
                  />
                </div>
                <div>
                  <label className="label">
                    <span className="label-text text-xs">{t("maxContextLength")}</span>
                  </label>
                  <input
                    className="input input-bordered input-sm w-full"
                    type="number"
                    min={1024}
                    step={1024}
                    value={activeProviderMaxContextLength}
                    onChange={(event: ChangeEvent<HTMLInputElement>) =>
                      updateProvider(activeProvider.id, {
                        maxContextLength: Number.parseInt(event.target.value || "0", 10),
                      })
                    }
                  />
                </div>
                <div>
                  <label className="label">
                    <span className="label-text text-xs">{t("customHeaders")}</span>
                  </label>
                  <textarea
                    className="textarea textarea-bordered textarea-sm h-24 w-full"
                    value={activeProviderCustomHeaders}
                    onChange={(event: ChangeEvent<HTMLTextAreaElement>) =>
                      updateProvider(activeProvider.id, { customHeaders: event.target.value })
                    }
                    placeholder='{"x-api-version":"2025-03-01"}'
                  />
                </div>
              </div>
            </div>
          </div>
        ) : null}

        {activeProvider ? (
          <div className="space-y-4">
            <div className="px-1 text-sm font-semibold">{t("advancedConfiguration")}</div>
            <div className="card card-border bg-base-100 shadow-sm">
              <div className="card-body gap-5 p-4">
                <div>
                  <div className="mb-1 flex items-center justify-between">
                    <span className="text-xs font-medium">{t("temperature")}</span>
                    <span className="text-xs font-mono text-primary">
                      {activeProvider.temperature.toFixed(1)}
                    </span>
                  </div>
                  <input
                    type="range"
                    className="range range-primary range-xs"
                    min={0}
                    max={2}
                    step={0.1}
                    value={activeProvider.temperature}
                    onChange={(event: ChangeEvent<HTMLInputElement>) =>
                      updateProvider(activeProvider.id, {
                        temperature: Number.parseFloat(event.target.value),
                      })
                    }
                  />
                </div>
                <div>
                  <div className="mb-1 flex items-center justify-between">
                    <span className="text-xs font-medium">{t("maxGenerationTokens")}</span>
                    <span className="text-xs font-mono text-primary">{activeProvider.maxTokens}</span>
                  </div>
                  <input
                    type="range"
                    className="range range-primary range-xs"
                    min={256}
                    max={8192}
                    step={256}
                    value={activeProvider.maxTokens}
                    onChange={(event: ChangeEvent<HTMLInputElement>) =>
                      updateProvider(activeProvider.id, {
                        maxTokens: Number.parseInt(event.target.value, 10),
                      })
                    }
                  />
                </div>
                <div>
                  <div className="mb-1 flex items-center justify-between">
                    <span className="text-xs font-medium">{t("outputTokenLimit")}</span>
                    <span className="text-xs font-mono text-primary">
                      {activeProvider.outputTokenLimit >= 1024
                        ? `${Math.round(activeProvider.outputTokenLimit / 1024)}K`
                        : activeProvider.outputTokenLimit}
                    </span>
                  </div>
                  <input
                    type="range"
                    className="range range-primary range-xs"
                    min={1024}
                    max={131072}
                    step={1024}
                    value={activeProvider.outputTokenLimit}
                    onChange={(event: ChangeEvent<HTMLInputElement>) =>
                      updateProvider(activeProvider.id, {
                        outputTokenLimit: Number.parseInt(event.target.value, 10),
                      })
                    }
                  />
                </div>
                <div>
                  <div className="mb-1 flex items-center justify-between">
                    <span className="text-xs font-medium">{t("maxDialogRounds")}</span>
                    <span className="text-xs font-mono text-primary">
                      {activeProvider.maxDialogRounds}
                    </span>
                  </div>
                  <input
                    type="range"
                    className="range range-primary range-xs"
                    min={1}
                    max={1000}
                    step={1}
                    value={activeProvider.maxDialogRounds}
                    onChange={(event: ChangeEvent<HTMLInputElement>) =>
                      updateProvider(activeProvider.id, {
                        maxDialogRounds: Number.parseInt(event.target.value, 10),
                      })
                    }
                  />
                </div>
              </div>
            </div>
          </div>
        ) : null}
      </div>

      {activeProvider ? (
        <div className="card card-border bg-base-100 shadow-sm">
          <div className="card-body p-4">
            <div className="mb-3 flex items-center gap-2">
              <h3 className="text-base font-bold">{t("availableModels")}</h3>
              <span className="badge badge-ghost badge-sm">{sortedModels.length}</span>
            </div>
            <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
              {sortedModels.map((model) => (
                <div key={model} className="rounded-xl border border-base-content/10 p-3">
                  <div className="mb-2 flex items-center justify-between gap-2">
                    <div className="truncate text-sm font-semibold">{model}</div>
                    <span className="badge badge-success badge-xs">{t("available")}</span>
                  </div>
                  <div className="text-xs text-base-content/60">{activeProvider.rigProviderType}</div>
                  <div className="mt-2 text-xs text-base-content/70">
                    {t("maxContextLength")}: {activeProviderMaxContextLength.toLocaleString()}
                  </div>
                  <div className="mt-2 flex flex-wrap gap-1">
                    {providerModules(activeProvider.rigProviderType).map((tag) => (
                      <span key={`${model}-${tag}`} className="badge badge-primary badge-xs">
                        {tag}
                      </span>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}

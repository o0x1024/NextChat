import { type ChangeEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { useAppStore } from "../../../store/appStore";
import { refreshProviderModels, testProviderConnection } from "../../../lib/tauri";
import type { AIProviderConfig } from "../../../types";

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
            className={`flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-left transition-all duration-200 ${isSelected
                ? "bg-primary/10 text-primary border border-primary/20"
                : "hover:bg-base-300/60"
                }`}
            onClick={onClick}
        >
            <i className={`${provider.icon} w-5 text-center`} />
            <span className="font-medium text-sm">{provider.name}</span>
        </button>
    );
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

    const [showApiKey, setShowApiKey] = useState(false);
    const [testStatus, setTestStatus] = useState<"idle" | "testing" | "success" | "error">("idle");
    const [refreshStatus, setRefreshStatus] = useState<"idle" | "refreshing" | "success" | "error">("idle");
    const [refreshMessage, setRefreshMessage] = useState("");

    const updateProvider = (id: string, updates: Partial<AIProviderConfig>) => {
        const newProviders = providers.map((p) =>
            p.id === id ? { ...p, ...updates } : p
        );
        void updateSettings({ ...settings, providers: newProviders });
    };

    const activeProvider = providers.find((p) => p.id === selectedProviderId);

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
            setRefreshMessage(
                t("modelsRefreshed", {
                    count: updatedProvider.models.length,
                })
            );
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

    return (
        <div className="space-y-6">
            <div className="flex items-center justify-between">
                <h2 className="text-lg font-bold">{t("aiConfiguration")}</h2>
                <div className="flex items-center gap-2">
                    <span className="badge badge-ghost text-xs">{t("graphicalMode")}</span>
                </div>
            </div>

            <div className="grid grid-cols-1 gap-6 xl:grid-cols-[220px_1fr_1fr]">
                <div className="space-y-2">
                    <div className="text-sm font-semibold px-1">{t("aiProviders")}</div>
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

                {activeProvider && (
                    <div className="space-y-4">
                        <div className="text-sm font-semibold px-1">{t("basicConfiguration")}</div>
                        <div className="card card-border bg-base-100 shadow-sm">
                            <div className="card-body gap-4 p-4">
                                <div className="flex items-center justify-between">
                                    <span className="text-sm">{t("enableProvider", { name: activeProvider.name })}</span>
                                    <input
                                        type="checkbox"
                                        className="toggle toggle-primary toggle-sm"
                                        checked={activeProvider.enabled}
                                        onChange={(e: ChangeEvent<HTMLInputElement>) =>
                                            updateProvider(activeProvider.id, { enabled: e.target.checked })
                                        }
                                    />
                                </div>
                                <div className="divider my-0" />
                                <div>
                                    <label className="label">
                                        <span className="label-text text-xs">
                                            {t("rigProviderType")}
                                            <span className="badge badge-xs badge-primary ml-2">{t("rigApiFormat")}</span>
                                        </span>
                                    </label>
                                    <select
                                        className="select select-bordered select-sm w-full"
                                        value={activeProvider.rigProviderType}
                                        onChange={(e: ChangeEvent<HTMLSelectElement>) =>
                                            updateProvider(activeProvider.id, { rigProviderType: e.target.value })
                                        }
                                    >
                                        <option value="OpenAI">OpenAI</option>
                                        <option value="Anthropic">Anthropic</option>
                                        <option value="DeepSeek">DeepSeek</option>
                                        <option value="Gemini">Gemini</option>
                                        <option value="Groq">Groq</option>
                                        <option value="Cohere">Cohere</option>
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
                                            onChange={(e: ChangeEvent<HTMLInputElement>) =>
                                                updateProvider(activeProvider.id, { apiKey: e.target.value })
                                            }
                                        />
                                        <button className="btn btn-sm btn-ghost join-item" onClick={() => setShowApiKey(!showApiKey)}>
                                            <i className={showApiKey ? "fas fa-eye-slash" : "fas fa-eye"} />
                                        </button>
                                        <button
                                            className={`btn btn-sm join-item ${testStatus === "testing" ? "btn-disabled" : testStatus === "success" ? "btn-success" : testStatus === "error" ? "btn-error" : "btn-ghost"}`}
                                            onClick={handleTestConnection}
                                        >
                                            {testStatus === "testing" ? <span className="loading loading-spinner loading-xs" /> : testStatus === "success" ? "✓" : testStatus === "error" ? "✕" : null}
                                            {t("testConnection")}
                                        </button>
                                    </div>
                                </div>
                                <div>
                                    <label className="label">
                                        <span className="label-text text-xs">{t("defaultModel")}</span>
                                        <button
                                            className={`btn btn-ghost btn-xs ${refreshStatus === "refreshing" ? "btn-disabled" : ""}`}
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
                                        onChange={(e: ChangeEvent<HTMLSelectElement>) =>
                                            updateProvider(activeProvider.id, { defaultModel: e.target.value })
                                        }
                                    >
                                        {activeProvider.models.map((model) => (
                                            <option key={model} value={model}>{model}</option>
                                        ))}
                                    </select>
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
                                        onChange={(e: ChangeEvent<HTMLInputElement>) =>
                                            updateProvider(activeProvider.id, { baseUrl: e.target.value })
                                        }
                                    />
                                </div>
                            </div>
                        </div>
                    </div>
                )}

                {activeProvider && (
                    <div className="space-y-4">
                        <div className="text-sm font-semibold px-1">{t("advancedConfiguration")}</div>
                        <div className="card card-border bg-base-100 shadow-sm">
                            <div className="card-body gap-5 p-4">
                                <div>
                                    <div className="flex items-center justify-between mb-1">
                                        <span className="text-xs font-medium">{t("temperature")}</span>
                                        <span className="text-xs font-mono text-primary">{activeProvider.temperature.toFixed(1)}</span>
                                    </div>
                                    <input
                                        type="range"
                                        className="range range-primary range-xs"
                                        min={0}
                                        max={2}
                                        step={0.1}
                                        value={activeProvider.temperature}
                                        onChange={(e: ChangeEvent<HTMLInputElement>) =>
                                            updateProvider(activeProvider.id, { temperature: Number.parseFloat(e.target.value) })
                                        }
                                    />
                                </div>
                                <div>
                                    <div className="flex items-center justify-between mb-1">
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
                                        onChange={(e: ChangeEvent<HTMLInputElement>) =>
                                            updateProvider(activeProvider.id, { maxTokens: Number.parseInt(e.target.value, 10) })
                                        }
                                    />
                                </div>
                                <div>
                                    <div className="flex items-center justify-between mb-1">
                                        <span className="text-xs font-medium">{t("outputTokenLimit")}</span>
                                        <span className="text-xs font-mono text-primary">
                                            {activeProvider.outputTokenLimit >= 1024 ? `${Math.round(activeProvider.outputTokenLimit / 1024)}K` : activeProvider.outputTokenLimit}
                                        </span>
                                    </div>
                                    <input
                                        type="range"
                                        className="range range-primary range-xs"
                                        min={1024}
                                        max={131072}
                                        step={1024}
                                        value={activeProvider.outputTokenLimit}
                                        onChange={(e: ChangeEvent<HTMLInputElement>) =>
                                            updateProvider(activeProvider.id, { outputTokenLimit: Number.parseInt(e.target.value, 10) })
                                        }
                                    />
                                </div>
                                <div>
                                    <div className="flex items-center justify-between mb-1">
                                        <span className="text-xs font-medium">{t("maxDialogRounds")}</span>
                                        <span className="text-xs font-mono text-primary">{activeProvider.maxDialogRounds}</span>
                                    </div>
                                    <input
                                        type="range"
                                        className="range range-primary range-xs"
                                        min={1}
                                        max={1000}
                                        step={1}
                                        value={activeProvider.maxDialogRounds}
                                        onChange={(e: ChangeEvent<HTMLInputElement>) =>
                                            updateProvider(activeProvider.id, { maxDialogRounds: Number.parseInt(e.target.value, 10) })
                                        }
                                    />
                                </div>
                            </div>
                        </div>
                    </div>
                )}
            </div>
        </div>
    );
}

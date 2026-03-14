import { useTranslation } from "react-i18next";
import { useState } from "react";
import { useAppStore } from "../../../store/appStore";
import { testProviderConnection } from "../../../lib/tauri";

export function NetworkSettings() {
    const { t } = useTranslation();
    const { settings, updateSettings, showToast } = useAppStore();
    const { globalConfig, providers } = settings;
    const [testStatus, setTestStatus] = useState<"idle" | "testing" | "success" | "error">("idle");

    const updateProxy = (value: string) => {
        void updateSettings({
            ...settings,
            globalConfig: {
                ...globalConfig,
                proxyUrl: value
            }
        }).then(() => {
            showToast(t("settingsSaved"), 'success');
        });
    };

    async function handleTestProxyConnection() {
        if (providers.length === 0) {
            showToast(t("noProvidersAvailable") || "No providers available for testing", 'error');
            return;
        }

        // Use the first enabled provider for testing, or the first provider if none are enabled
        const providerToTest = providers.find(p => p.enabled) || providers[0];
        if (!providerToTest) {
            showToast(t("noProvidersAvailable") || "No providers available for testing", 'error');
            return;
        }

        setTestStatus("testing");
        try {
            await testProviderConnection(providerToTest);
            setTestStatus("success");
            showToast(t("connectionSuccessful") || "Connection successful", 'success');
        } catch (error) {
            console.error("Proxy connection test failed:", error);
            setTestStatus("error");
            const errorMsg = error instanceof Error ? error.message : "Connection failed";
            showToast(errorMsg, 'error');
        } finally {
            setTimeout(() => setTestStatus("idle"), 3000);
        }
    }

    return (
        <div className="space-y-6 animate-in fade-in slide-in-from-right-2 duration-300">
            <div>
                <h2 className="text-lg font-bold flex items-center gap-2 mb-4">
                    <i className="fas fa-network-wired text-primary" />
                    {t("settingsNetwork")}
                </h2>

                <div className="space-y-4">
                    <div className="card card-border bg-base-100 shadow-sm border-base-content/5">
                        <div className="card-body p-6 space-y-4">
                            <div className="form-control">
                                <label className="label">
                                    <span className="label-text font-bold text-xs uppercase opacity-60">{t("proxySettings")}</span>
                                </label>
                                <div className="flex gap-2">
                                    <input
                                        type="text"
                                        className="input input-bordered input-sm flex-1 bg-base-200/50"
                                        placeholder="http://127.0.0.1:7890"
                                        value={globalConfig.proxyUrl}
                                        onChange={(e) => updateProxy(e.target.value)}
                                    />
                                    <button
                                        className={`btn btn-sm join-item ${testStatus === "testing" ? "btn-disabled" : testStatus === "success" ? "btn-success" : testStatus === "error" ? "btn-error" : "btn-ghost"}`}
                                        type="button"
                                        onClick={handleTestProxyConnection}
                                        disabled={testStatus === "testing"}
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
                                <label className="label">
                                    <span className="label-text-alt opacity-40 text-[10px]">{t("proxyDesc")}</span>
                                </label>
                            </div>

                            <div className="divider opacity-5 my-0"></div>

                            <div className="alert bg-primary/5 border-primary/10 text-[10px] py-3">
                                <i className="fas fa-shield-alt text-primary" />
                                <span className="opacity-70 leading-relaxed font-medium">
                                    {t("proxyHint")}
                                </span>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    );
}

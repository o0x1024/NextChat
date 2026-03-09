import { useTranslation } from "react-i18next";
import { useAppStore } from "../../../store/appStore";

export function SecuritySettings() {
    const { t } = useTranslation();
    const { settings, updateSettings } = useAppStore();
    const { globalConfig } = settings;

    const toggleSecuritySetting = (key: 'maskApiKeys' | 'enableAuditLog') => {
        void updateSettings({
            ...settings,
            globalConfig: {
                ...globalConfig,
                [key]: !globalConfig[key]
            }
        });
    };

    return (
        <div className="space-y-6 animate-in fade-in slide-in-from-right-2 duration-300">
            <div>
                <h2 className="text-lg font-bold flex items-center gap-2 mb-4">
                    <i className="fas fa-shield-halved text-primary" />
                    {t("settingsSecurity")}
                </h2>

                <div className="space-y-4">
                    <div className="card card-border bg-base-100 shadow-sm border-base-content/5">
                        <div className="card-body p-6">
                            <div className="flex items-center justify-between">
                                <div className="space-y-0.5">
                                    <div className="text-sm font-bold flex items-center gap-2">
                                        <i className="fas fa-eye-slash text-base-content/40" />
                                        {t("maskApiKeys")}
                                    </div>
                                    <div className="text-xs text-base-content/50">{t("maskApiKeysDesc")}</div>
                                </div>
                                <input
                                    type="checkbox"
                                    className="toggle toggle-primary"
                                    checked={globalConfig.maskApiKeys}
                                    onChange={() => toggleSecuritySetting('maskApiKeys')}
                                />
                            </div>

                            <div className="divider opacity-5 my-2"></div>

                            <div className="flex items-center justify-between">
                                <div className="space-y-0.5">
                                    <div className="text-sm font-bold flex items-center gap-2">
                                        <i className="fas fa-history text-base-content/40" />
                                        {t("enableAuditLog")}
                                    </div>
                                    <div className="text-xs text-base-content/50">{t("enableAuditLogDesc")}</div>
                                </div>
                                <input
                                    type="checkbox"
                                    className="toggle toggle-primary"
                                    checked={globalConfig.enableAuditLog}
                                    onChange={() => toggleSecuritySetting('enableAuditLog')}
                                />
                            </div>
                        </div>
                    </div>

                    <div className="alert bg-base-200/50 border-base-content/5 text-xs">
                        <i className="fas fa-info-circle text-primary" />
                        <div>
                            <h3 className="font-bold opacity-70">{t("dataLocalOnly")}</h3>
                            <div className="opacity-50 mt-1">{t("dataLocalOnlyDesc")}</div>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    );
}

import { useTranslation } from "react-i18next";
import { usePreferencesStore } from "../../../store/preferencesStore";
import { daisyThemes, type ThemeMode } from "../../../constants/themes";

export function SystemSettings() {
    const { t } = useTranslation();
    const { fontSize, setFontSize, componentSpacing, setComponentSpacing, theme, setTheme } = usePreferencesStore();

    return (
        <div className="space-y-6 animate-in fade-in slide-in-from-right-2 duration-300">
            <div>
                <h2 className="text-lg font-bold flex items-center gap-2 mb-4">
                    <i className="fas fa-desktop text-primary" />
                    {t("settingsSystem")}
                </h2>

                <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                    <div className="space-y-4">
                        <div className="text-sm font-semibold opacity-50 uppercase tracking-widest">{t("interfaceSettings")}</div>
                        <div className="card card-border bg-base-100 shadow-sm border-base-content/5">
                            <div className="card-body p-5 gap-6">
                                <div className="space-y-3">
                                    <div className="flex items-center justify-between">
                                        <label className="text-xs font-bold flex items-center gap-2">
                                            <i className="fas fa-font" />
                                            {t("fontSize")}
                                        </label>
                                        <span className="badge badge-primary badge-sm font-mono">{fontSize}px</span>
                                    </div>
                                    <div className="flex items-center gap-3">
                                        <span className="text-[10px] opacity-40">A</span>
                                        <input
                                            type="range"
                                            min="12"
                                            max="20"
                                            step="1"
                                            className="range range-primary range-xs flex-1"
                                            value={fontSize}
                                            onChange={(e) => setFontSize(parseInt(e.target.value))}
                                        />
                                        <span className="text-sm opacity-40">A</span>
                                    </div>
                                </div>

                                <div className="space-y-3">
                                    <div className="flex items-center justify-between">
                                        <label className="text-xs font-bold flex items-center gap-2">
                                            <i className="fas fa-arrows-left-right-to-line" />
                                            {t("componentSpacing")}
                                        </label>
                                        <span className="badge badge-primary badge-sm font-mono">{componentSpacing}px</span>
                                    </div>
                                    <div className="flex items-center gap-3 text-primary">
                                        <i className="fas fa-compress text-[10px] opacity-40" />
                                        <input
                                            type="range"
                                            min="0"
                                            max="12"
                                            step="1"
                                            className="range range-primary range-xs flex-1"
                                            value={componentSpacing}
                                            onChange={(e) => setComponentSpacing(parseInt(e.target.value))}
                                        />
                                        <i className="fas fa-expand text-sm opacity-40" />
                                    </div>
                                </div>
                            </div>
                        </div>
                    </div>

                    <div className="space-y-4">
                        <div className="text-sm font-semibold opacity-50 uppercase tracking-widest">{t("themeSettings")}</div>
                        <div className="card card-border bg-base-100 shadow-sm border-base-content/5">
                            <div className="card-body p-5">
                                <div className="grid grid-cols-2 gap-2 max-h-[220px] overflow-y-auto pr-2 custom-scrollbar">
                                    {daisyThemes.map((themeName) => (
                                        <button
                                            key={themeName}
                                            onClick={() => setTheme(themeName as ThemeMode)}
                                            className={`flex items-center justify-between p-2.5 rounded-xl border text-xs transition-all ${theme === themeName
                                                ? "border-primary bg-primary/5 text-primary shadow-sm"
                                                : "border-base-content/5 hover:border-base-content/20 bg-base-200/50"
                                                }`}
                                        >
                                            <span className="capitalize truncate mr-2">{themeName}</span>
                                            <div className="flex gap-0.5">
                                                <div className="w-2 h-2 rounded-full bg-primary" />
                                                <div className="w-2 h-2 rounded-full bg-secondary" />
                                            </div>
                                        </button>
                                    ))}
                                </div>
                            </div>
                        </div>
                    </div>
                </div>

                <div className="mt-8 text-xs text-base-content/40 bg-base-200/50 p-4 rounded-xl border border-base-content/5 flex items-start gap-3">
                    <i className="fas fa-info-circle mt-0.5" />
                    <p>{t("systemSettingsPlaceholder")}</p>
                </div>
            </div>
        </div>
    );
}

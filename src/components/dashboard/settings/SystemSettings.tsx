import { useTranslation } from "react-i18next";
import {
    COMPONENT_SCALE_MAX,
    COMPONENT_SCALE_MIN,
    COMPONENT_SCALE_STEP,
    COMPONENT_SPACING_MAX,
    COMPONENT_SPACING_MIN,
    COMPONENT_SPACING_STEP,
    FONT_SIZE_MAX,
    FONT_SIZE_MIN,
    FONT_SIZE_STEP,
} from "../../../constants/preferences";
import { usePreferencesStore } from "../../../store/preferencesStore";
import { useAppStore } from "../../../store/appStore";
import { daisyThemes, type ThemeMode } from "../../../constants/themes";
import { ContinuousRangeControl } from "./ContinuousRangeControl";

export function SystemSettings() {
    const { t } = useTranslation();
    const {
        fontSize,
        setFontSize,
        componentScale,
        setComponentScale,
        componentSpacing,
        setComponentSpacing,
        theme,
        setTheme,
    } = usePreferencesStore();
    const { showToast } = useAppStore();

    const notifySaved = () => {
        showToast(t("settingsSaved"), 'success');
    };

    const handleFontSizeChange = (value: number) => {
        setFontSize(value);
    };

    const handleComponentScaleChange = (value: number) => {
        setComponentScale(value);
    };

    const handleSpacingChange = (value: number) => {
        setComponentSpacing(value);
    };

    const handleThemeChange = (themeName: ThemeMode) => {
        setTheme(themeName);
        notifySaved();
    };

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
                                <ContinuousRangeControl
                                    iconClass="fas fa-font"
                                    label={t("fontSize")}
                                    value={fontSize}
                                    min={FONT_SIZE_MIN}
                                    max={FONT_SIZE_MAX}
                                    step={FONT_SIZE_STEP}
                                    badge={`${fontSize}px`}
                                    minIndicator="A"
                                    maxIndicator="A"
                                    onChange={handleFontSizeChange}
                                    onCommit={notifySaved}
                                />

                                <ContinuousRangeControl
                                    iconClass="fas fa-expand"
                                    label={t("componentSize")}
                                    value={componentScale}
                                    min={COMPONENT_SCALE_MIN}
                                    max={COMPONENT_SCALE_MAX}
                                    step={COMPONENT_SCALE_STEP}
                                    badge={`${Math.round(componentScale * 100)}%`}
                                    minIndicator={<i className="fas fa-minimize text-[10px]" />}
                                    maxIndicator={<i className="fas fa-maximize" />}
                                    onChange={handleComponentScaleChange}
                                    onCommit={notifySaved}
                                />

                                <ContinuousRangeControl
                                    iconClass="fas fa-arrows-left-right-to-line"
                                    label={t("componentSpacing")}
                                    value={componentSpacing}
                                    min={COMPONENT_SPACING_MIN}
                                    max={COMPONENT_SPACING_MAX}
                                    step={COMPONENT_SPACING_STEP}
                                    badge={`${componentSpacing}px`}
                                    minIndicator={<i className="fas fa-compress text-[10px]" />}
                                    maxIndicator={<i className="fas fa-expand" />}
                                    onChange={handleSpacingChange}
                                    onCommit={notifySaved}
                                />
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
                                            onClick={() => handleThemeChange(themeName as ThemeMode)}
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

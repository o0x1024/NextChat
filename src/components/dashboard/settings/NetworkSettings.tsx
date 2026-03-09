import { useTranslation } from "react-i18next";

export function NetworkSettings() {
    const { t } = useTranslation();
    return (
        <div className="space-y-6 animate-in fade-in slide-in-from-right-2 duration-300">
            <div>
                <h2 className="text-lg font-bold flex items-center gap-2 mb-4">
                    <i className="fas fa-network-wired text-primary" />
                    {t("settingsNetwork")}
                </h2>
                <div className="card card-border bg-base-100 shadow-sm border-base-content/5">
                    <div className="card-body p-8 items-center text-center gap-4">
                        <div className="w-16 h-16 rounded-full bg-primary/10 flex items-center justify-center mb-2">
                            <i className="fas fa-cloud text-primary text-2xl" />
                        </div>
                        <h3 className="text-xl font-bold">{t("comingSoonTitle")}</h3>
                        <p className="text-base-content/60 text-sm max-w-sm">
                            {t("comingSoonDesc")}
                        </p>
                    </div>
                </div>
            </div>
        </div>
    );
}

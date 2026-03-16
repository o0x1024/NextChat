import { useTranslation } from "react-i18next";
import { SettingsSidebar } from "./settings/SettingsSidebar";
import { AISettings } from "./settings/AISettings";
import { SystemSettings } from "./settings/SystemSettings";
import { SecuritySettings } from "./settings/SecuritySettings";
import { NetworkSettings } from "./settings/NetworkSettings";

export type SettingsTabType = "ai" | "system" | "security" | "network";

interface SettingsWorkspaceProps {
    settingsTab: SettingsTabType;
    onSettingsTabChange: (tab: SettingsTabType) => void;
}

export function SettingsWorkspace({
    settingsTab,
    onSettingsTabChange,
}: SettingsWorkspaceProps) {
    const { t } = useTranslation();

    const tabs = [
        { id: "ai", label: t("settingsAIService"), icon: "fas fa-robot text-primary" },
        { id: "system", label: t("settingsSystem"), icon: "fas fa-desktop text-secondary" },
        { id: "security", label: t("settingsSecurity"), icon: "fas fa-shield-halved text-accent" },
        { id: "network", label: t("settingsNetwork"), icon: "fas fa-network-wired text-info" },
    ] as const;

    const renderContent = () => {
        switch (settingsTab) {
            case "ai":
                return <AISettings />;
            case "system":
                return <SystemSettings />;
            case "security":
                return <SecuritySettings />;
            case "network":
                return <NetworkSettings />;
            default:
                return null;
        }
    };

    return (
        <div className="flex h-full animate-in fade-in duration-500">
            <SettingsSidebar
                tabs={[...tabs]}
                activeTabId={settingsTab}
                onTabChange={(id) => onSettingsTabChange(id as SettingsTabType)}
            />

            <div className="flex-1 overflow-y-auto bg-base-200/30">
                <div className="mx-auto max-w-6xl p-8">
                    {renderContent()}
                </div>
            </div>
        </div>
    );
}

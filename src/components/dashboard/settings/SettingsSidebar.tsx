import { useTranslation } from "react-i18next";

interface SettingsTab {
    id: string;
    label: string;
    icon: string;
}

interface SettingsSidebarProps {
    tabs: SettingsTab[];
    activeTabId: string;
    onTabChange: (id: string) => void;
}

export function SettingsSidebar({ tabs, activeTabId, onTabChange }: SettingsSidebarProps) {
    return (
        <div className="w-48 shrink-0 border-r border-base-content/10 overflow-y-auto p-3 space-y-1">
            {tabs.map((tab) => (
                <button
                    key={tab.id}
                    className={`flex w-full items-center gap-2.5 rounded-lg px-3 py-2.5 text-left text-sm transition-all ${activeTabId === tab.id
                        ? "bg-primary text-primary-content font-semibold shadow-sm"
                        : "hover:bg-base-200"
                        }`}
                    onClick={() => onTabChange(tab.id)}
                >
                    <i className={`${tab.icon} w-4 text-center`} />
                    <span>{tab.label}</span>
                </button>
            ))}
        </div>
    );
}

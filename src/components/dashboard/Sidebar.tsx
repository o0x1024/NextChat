import { useTranslation } from "react-i18next";

export type ViewType = "agents" | "chats" | "tools" | "settings";

interface SidebarProps {
  activeView: ViewType;
  onViewChange: (view: ViewType) => void;
}

const menuItems: { id: ViewType; iconClass: string; labelKey: string }[] = [
  { id: "agents", iconClass: "fas fa-robot", labelKey: "agentManagement" },
  { id: "chats", iconClass: "fas fa-comments", labelKey: "chatManagement" },
  { id: "tools", iconClass: "fas fa-wrench", labelKey: "toolManagement" },
  { id: "settings", iconClass: "fas fa-cog", labelKey: "systemSettings" },
];

export function Sidebar({ activeView, onViewChange }: SidebarProps) {
  const { t } = useTranslation();

  return (
    <aside className="flex w-56 shrink-0 flex-col border-r border-base-content/10 bg-base-100">
      {/* Logo area */}
      <div className="flex items-center gap-2.5 border-b border-base-content/10 px-5 py-4">
        <div className="grid h-8 w-8 place-items-center rounded-lg bg-primary text-primary-content text-sm font-bold">
          N
        </div>
        <span className="text-sm font-bold tracking-tight">{t("appTitle")}</span>
      </div>

      {/* Menu */}
      <nav className="flex-1 overflow-y-auto px-3 py-4">
        <ul className="menu menu-sm gap-1">
          {menuItems.map((item) => (
            <li key={item.id}>
              <a
                className={`flex items-center gap-3 rounded-lg px-3 py-2.5 text-sm transition-all ${activeView === item.id
                    ? "bg-primary text-primary-content font-semibold shadow-sm"
                    : "hover:bg-base-200"
                  }`}
                onClick={() => onViewChange(item.id)}
              >
                <i className={`${item.iconClass} w-4 text-center`} />
                <span>{t(item.labelKey)}</span>
              </a>
            </li>
          ))}
        </ul>
      </nav>

      {/* Bottom */}
      <div className="border-t border-base-content/10 px-4 py-3">
        <div className="text-xs text-base-content/40 text-center">NextChat v0.1.0</div>
      </div>
    </aside>
  );
}

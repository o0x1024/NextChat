import { useTranslation } from "react-i18next";
import type { WorkGroup } from "../../types";

interface ChatGroupSidebarProps {
  workGroups: WorkGroup[];
  currentGroupId?: string;
  onSelectWorkGroup: (groupId: string) => void;
  onOpenCreateGroupModal: () => void;
  onCloseSidebar: () => void;
  onEditGroup: (group: WorkGroup) => void;
  onDeleteGroup: (group: WorkGroup) => void;
}

export function ChatGroupSidebar({
  workGroups,
  currentGroupId,
  onSelectWorkGroup,
  onOpenCreateGroupModal,
  onCloseSidebar,
  onEditGroup,
  onDeleteGroup,
}: ChatGroupSidebarProps) {
  const { t } = useTranslation();

  return (
    <div className="flex h-full min-h-0 flex-col border-r border-base-content/10">
      <div className="flex items-center justify-between border-b border-base-content/10 px-4 py-3">
        <h2 className="text-sm font-bold">{t("chatManagement")}</h2>
        <div className="flex items-center gap-1">
          <button className="btn btn-primary btn-xs" onClick={onOpenCreateGroupModal}>
            + {t("create")}
          </button>
          <button className="btn btn-ghost btn-xs" onClick={onCloseSidebar}>
            <i className="fas fa-indent" />
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-x-hidden overflow-y-auto">
        <ul className="menu menu-sm w-full gap-0.5 p-2">
          {workGroups.map((group) => {
            const isActive = currentGroupId === group.id;
            return (
              <li key={group.id} className="w-full overflow-hidden">
                <div
                  role="button"
                  tabIndex={0}
                  className={`flex w-full items-center gap-2 overflow-hidden rounded-lg py-2.5 transition-colors ${
                    isActive ? "bg-primary text-primary-content" : "hover:bg-base-200"
                  }`}
                  onClick={() => onSelectWorkGroup(group.id)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault();
                      onSelectWorkGroup(group.id);
                    }
                  }}
                >
                  <div
                    className={`grid h-8 w-8 shrink-0 place-items-center rounded-lg text-[10px] font-bold ${
                      isActive
                        ? "border border-primary-content/30 bg-primary-content/10 text-primary-content"
                        : "border border-primary/20 bg-primary/10 text-primary"
                    }`}
                  >
                    {group.name.slice(0, 2).toUpperCase()}
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium">{group.name}</div>
                    <div
                      className={`truncate text-xs ${
                        isActive ? "text-primary-content/75" : "text-base-content/50"
                      }`}
                    >
                      {group.goal}
                    </div>
                  </div>
                  <div className="flex shrink-0 items-center gap-1">
                    <button
                      type="button"
                      className={`btn btn-ghost btn-xs ${
                        isActive ? "text-primary-content hover:bg-primary-content/20" : ""
                      }`}
                      title={t("edit")}
                      onClick={(event) => {
                        event.stopPropagation();
                        onEditGroup(group);
                      }}
                    >
                      <i className="fas fa-pen" />
                    </button>
                    <button
                      type="button"
                      className={`btn btn-ghost btn-xs ${
                        isActive ? "text-primary-content hover:bg-primary-content/20" : "text-error"
                      }`}
                      title={t("delete")}
                      onClick={(event) => {
                        event.stopPropagation();
                        onDeleteGroup(group);
                      }}
                    >
                      <i className="fas fa-trash" />
                    </button>
                  </div>
                  <span
                    className={`badge badge-xs shrink-0 ${
                      isActive
                        ? "border-primary-content/30 bg-primary-content/15 text-primary-content"
                        : "badge-ghost"
                    }`}
                  >
                    {group.kind === "persistent" ? "P" : "E"}
                  </span>
                </div>
              </li>
            );
          })}
          {workGroups.length === 0 && (
            <li className="py-4 text-center text-sm text-base-content/50">
              {t("noWorkGroupsYet")}
            </li>
          )}
        </ul>
      </div>
    </div>
  );
}

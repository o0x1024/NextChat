import { useTranslation } from "react-i18next";

interface AgentBulkActionsBarProps {
  selectedCount: number;
  onClearSelection: () => void;
  onDelete: () => void;
  onAddToGroup: () => void;
  onBulkEdit: () => void;
  onCopyConfig: () => void;
  onExportConfig: () => void;
}

export function AgentBulkActionsBar({
  selectedCount,
  onClearSelection,
  onDelete,
  onAddToGroup,
  onBulkEdit,
  onCopyConfig,
  onExportConfig,
}: AgentBulkActionsBarProps) {
  const { t } = useTranslation();

  if (selectedCount === 0) {
    return null;
  }

  return (
    <>
      <span className="rounded-lg bg-base-200 px-3 py-2 text-xs font-semibold text-base-content/70">
        {t("selectedAgentsCount", { count: selectedCount })}
      </span>
      <button className="btn btn-ghost btn-sm" onClick={onClearSelection}>
        {t("clearSelection")}
      </button>
      <button className="btn btn-outline btn-sm gap-2" onClick={onAddToGroup}>
        <i className="fas fa-users text-xs" />
        {t("bulkAddToWorkGroup")}
      </button>
      <button className="btn btn-outline btn-sm gap-2" onClick={onBulkEdit}>
        <i className="fas fa-sliders text-xs" />
        {t("bulkEditAgents")}
      </button>
      <button className="btn btn-ghost btn-sm gap-2" onClick={onCopyConfig}>
        <i className="fas fa-copy text-xs" />
        {t("copySelectedConfig")}
      </button>
      <button className="btn btn-ghost btn-sm gap-2" onClick={onExportConfig}>
        <i className="fas fa-file-export text-xs" />
        {t("exportSelectedConfig")}
      </button>
      <button className="btn btn-error btn-sm gap-2" onClick={onDelete}>
        <i className="fas fa-trash-alt text-xs" />
        {t("deleteSelectedAgents")}
      </button>
    </>
  );
}

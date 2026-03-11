import { useTranslation } from "react-i18next";
import type { WorkGroup } from "../../types";

interface AgentBulkAddToGroupModalProps {
  open: boolean;
  submitting: boolean;
  selectedCount: number;
  workGroups: WorkGroup[];
  selectedWorkGroupId: string;
  onClose: () => void;
  onSelectWorkGroup: (workGroupId: string) => void;
  onSubmit: () => void;
}

export function AgentBulkAddToGroupModal({
  open,
  submitting,
  selectedCount,
  workGroups,
  selectedWorkGroupId,
  onClose,
  onSelectWorkGroup,
  onSubmit,
}: AgentBulkAddToGroupModalProps) {
  const { t } = useTranslation();

  if (!open) {
    return null;
  }

  return (
    <dialog className="modal modal-open bg-base-300/40 backdrop-blur-md" onClick={onClose}>
      <div className="modal-box max-w-lg rounded-2xl border border-base-content/10 shadow-2xl" onClick={(event) => event.stopPropagation()}>
        <h3 className="text-lg font-bold">{t("bulkAddToWorkGroup")}</h3>
        <p className="mt-2 text-sm text-base-content/60">{t("bulkAddToWorkGroupHint", { count: selectedCount })}</p>
        <div className="mt-4 form-control">
          <label className="label">
            <span className="label-text text-xs font-bold text-base-content/60 uppercase">{t("workGroups")}</span>
          </label>
          <select
            className="select select-bordered w-full bg-base-200/50"
            value={selectedWorkGroupId}
            onChange={(event) => onSelectWorkGroup(event.target.value)}
          >
            <option value="">{t("selectWorkGroup")}</option>
            {workGroups.map((workGroup) => (
              <option key={workGroup.id} value={workGroup.id}>
                {workGroup.name}
              </option>
            ))}
          </select>
        </div>
        <div className="modal-action">
          <button className="btn btn-ghost" disabled={submitting} onClick={onClose}>
            {t("cancel")}
          </button>
          <button className="btn btn-primary" disabled={submitting || !selectedWorkGroupId} onClick={onSubmit}>
            {submitting ? t("processingBulkAction") : t("confirmBulkAddToWorkGroup")}
          </button>
        </div>
      </div>
    </dialog>
  );
}

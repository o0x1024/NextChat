import { useTranslation } from "react-i18next";
import type { AgentProfile } from "../../types";

interface AgentDeleteConfirmModalProps {
    open: boolean;
    agents: AgentProfile[];
    onClose: () => void;
    onConfirm: () => void;
}

export function AgentDeleteConfirmModal({
    open,
    agents,
    onClose,
    onConfirm,
}: AgentDeleteConfirmModalProps) {
    const { t } = useTranslation();

    if (!open) {
        return null;
    }

    return (
        <dialog className="modal modal-open animate-in zoom-in duration-200" onClick={onClose}>
            <div className="modal-box max-w-sm rounded-2xl border border-error/20" onClick={(event) => event.stopPropagation()}>
                <div className="flex flex-col items-center text-center gap-4 py-4">
                    <div className="w-16 h-16 rounded-full bg-error/10 flex items-center justify-center mb-2">
                        <i className="fas fa-exclamation-triangle text-error text-2xl" />
                    </div>
                    <h3 className="text-xl font-bold">{t("areYouSure")}</h3>
                    <p className="text-base-content/60 text-sm">
                        {agents.length > 1
                            ? t("deleteAgentsConfirmHint", { count: agents.length })
                            : t("deleteAgentConfirmHint")}
                    </p>
                    <p className="text-xs text-base-content/40">
                        {agents.slice(0, 4).map((agent) => agent.name).join(" , ")}
                    </p>
                    <div className="flex gap-3 w-full mt-4">
                        <button className="btn btn-ghost flex-1" onClick={onClose}>{t("cancel")}</button>
                        <button className="btn btn-error flex-1" onClick={onConfirm}>{t("delete")}</button>
                    </div>
                </div>
            </div>
        </dialog>
    );
}

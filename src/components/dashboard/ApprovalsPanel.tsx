import { useTranslation } from "react-i18next";
import type { ToolManifest, ToolRun } from "../../types";

interface ApprovalsPanelProps {
  currentApprovals: ToolRun[];
  tools: ToolManifest[];
  onApproveRun: (toolRunId: string, approved: boolean) => void;
}

export function ApprovalsPanel({
  currentApprovals,
  tools,
  onApproveRun,
}: ApprovalsPanelProps) {
  const { t } = useTranslation();

  return (
    <section className="card card-border bg-base-100">
      <div className="card-body gap-4">
        <div className="flex items-center justify-between">
          <h2 className="card-title">{t("approvals")}</h2>
          <span className="badge badge-warning">
            {currentApprovals.length}
          </span>
        </div>

        <ul className="list gap-3">
          {currentApprovals.map((run) => {
            const tool = tools.find((item) => item.id === run.toolId);
            return (
              <li key={run.id} className="list-row rounded-box bg-warning/10">
                <div className="min-w-0 flex-1">
                  <div className="mb-1 flex items-start justify-between gap-2">
                    <strong className="truncate">{tool?.name ?? run.toolId}</strong>
                    <span className="badge badge-warning shrink-0">{t("approval")}</span>
                  </div>
                  <p className="text-sm">{t("toolRunNeedsApproval", { id: run.id.slice(0, 8) })}</p>
                </div>
                <div className="join self-start">
                  <button
                    className="btn btn-primary btn-sm join-item"
                    onClick={() => onApproveRun(run.id, true)}
                  >
                    {t("approve")}
                  </button>
                  <button
                    className="btn btn-soft btn-sm join-item"
                    onClick={() => onApproveRun(run.id, false)}
                  >
                    {t("reject")}
                  </button>
                </div>
              </li>
            );
          })}

          {currentApprovals.length === 0 ? (
            <li className="list-row rounded-box">
              <div className="alert alert-soft">
                <span className="text-sm">{t("noPendingApprovals")}</span>
              </div>
            </li>
          ) : null}
        </ul>
      </div>
    </section>
  );
}

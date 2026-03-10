import type { ChangeEvent, Dispatch, FormEvent, SetStateAction } from "react";
import { useTranslation } from "react-i18next";
import type {
  AgentProfile,
  CreateWorkGroupInput,
  UpdateWorkGroupInput,
  WorkGroup,
} from "../../types";

interface WorkGroupDialogsProps {
  createModalOpen: boolean;
  editModalOpen: boolean;
  deleteTargetGroup: WorkGroup | null;
  deletingGroup: boolean;
  clearHistoryTargetGroup: WorkGroup | null;
  clearingHistory: boolean;
  currentGroup?: WorkGroup;
  agents: AgentProfile[];
  groupForm: CreateWorkGroupInput;
  editGroupForm: UpdateWorkGroupInput;
  directoryPickerError: string | null;
  onSetCreateModalOpen: Dispatch<SetStateAction<boolean>>;
  onSetEditModalOpen: Dispatch<SetStateAction<boolean>>;
  onSetDeleteTargetGroup: Dispatch<SetStateAction<WorkGroup | null>>;
  onSetClearHistoryTargetGroup: Dispatch<SetStateAction<WorkGroup | null>>;
  onSetGroupForm: Dispatch<SetStateAction<CreateWorkGroupInput>>;
  onSetEditGroupForm: Dispatch<SetStateAction<UpdateWorkGroupInput>>;
  onHandleCreateGroup: (event: FormEvent<HTMLFormElement>) => Promise<void>;
  onHandleUpdateGroup: (event: FormEvent<HTMLFormElement>) => Promise<void>;
  onHandlePickWorkingDirectory: (mode: "create" | "edit") => Promise<void>;
  onHandleToggleCreateMember: (agentId: string) => void;
  onConfirmDeleteGroup: () => Promise<void>;
  onConfirmClearHistory: () => Promise<void>;
}

export function WorkGroupDialogs({
  createModalOpen,
  editModalOpen,
  deleteTargetGroup,
  deletingGroup,
  clearHistoryTargetGroup,
  clearingHistory,
  currentGroup,
  agents,
  groupForm,
  editGroupForm,
  directoryPickerError,
  onSetCreateModalOpen,
  onSetEditModalOpen,
  onSetDeleteTargetGroup,
  onSetClearHistoryTargetGroup,
  onSetGroupForm,
  onSetEditGroupForm,
  onHandleCreateGroup,
  onHandleUpdateGroup,
  onHandlePickWorkingDirectory,
  onHandleToggleCreateMember,
  onConfirmDeleteGroup,
  onConfirmClearHistory,
}: WorkGroupDialogsProps) {
  const { t } = useTranslation();

  return (
    <>
      {createModalOpen && (
        <dialog className="modal modal-open" onClick={() => onSetCreateModalOpen(false)}>
          <div className="modal-box" onClick={(event) => event.stopPropagation()}>
            <button
              className="btn btn-sm btn-circle btn-ghost absolute right-3 top-3"
              onClick={() => onSetCreateModalOpen(false)}
            >
              ✕
            </button>
            <h3 className="mb-4 text-lg font-bold">{t("createWorkGroup")}</h3>
            <form className="space-y-3" onSubmit={(event) => void onHandleCreateGroup(event)}>
              <input
                className="input input-bordered input-sm w-full"
                placeholder={t("name")}
                required
                value={groupForm.name}
                onChange={(event: ChangeEvent<HTMLInputElement>) =>
                  onSetGroupForm((form) => ({ ...form, name: event.target.value }))
                }
              />
              <textarea
                className="textarea textarea-bordered textarea-sm w-full"
                placeholder={t("sharedGoal")}
                rows={2}
                value={groupForm.goal}
                onChange={(event: ChangeEvent<HTMLTextAreaElement>) =>
                  onSetGroupForm((form) => ({ ...form, goal: event.target.value }))
                }
              />
              <div className="join w-full">
                <input
                  className="input input-bordered input-sm join-item w-full"
                  placeholder={t("workingDirectory")}
                  required
                  value={groupForm.workingDirectory}
                  onChange={(event: ChangeEvent<HTMLInputElement>) =>
                    onSetGroupForm((form) => ({ ...form, workingDirectory: event.target.value }))
                  }
                />
                <button
                  className="btn btn-outline btn-sm join-item"
                  type="button"
                  title={t("chooseFolder")}
                  aria-label={t("chooseFolder")}
                  onClick={() => void onHandlePickWorkingDirectory("create")}
                >
                  <i className="fas fa-folder-open" />
                </button>
              </div>
              {directoryPickerError && (
                <div className="text-xs text-error">{directoryPickerError}</div>
              )}
              <select
                className="select select-bordered select-sm w-full"
                value={groupForm.kind}
                onChange={(event: ChangeEvent<HTMLSelectElement>) =>
                  onSetGroupForm((form) => ({
                    ...form,
                    kind: event.target.value as CreateWorkGroupInput["kind"],
                  }))
                }
              >
                <option value="persistent">{t("persistent")}</option>
                <option value="ephemeral">{t("ephemeral")}</option>
              </select>
              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <div className="text-xs font-semibold uppercase tracking-wide text-base-content/60">
                    {t("members")}
                  </div>
                  <div className="text-xs text-base-content/60">
                    {(groupForm.memberAgentIds ?? []).length}/{agents.length}
                  </div>
                </div>
                {agents.length === 0 ? (
                  <div className="rounded-box border border-base-content/10 px-3 py-2 text-xs text-base-content/60">
                    {t("noAgentsYet")}
                  </div>
                ) : (
                  <div className="max-h-48 space-y-2 overflow-y-auto rounded-box border border-base-content/10 p-2">
                    {agents.map((agent) => {
                      const isSelected = (groupForm.memberAgentIds ?? []).includes(agent.id);
                      return (
                        <div
                          key={agent.id}
                          className="flex items-center justify-between rounded-box bg-base-200 px-3 py-2"
                        >
                          <div className="flex items-center gap-2">
                            <div className="grid h-7 w-7 place-items-center rounded-btn bg-primary/10 text-[10px] font-bold text-primary">
                              {agent.avatar}
                            </div>
                            <div>
                              <div className="text-sm font-medium">{agent.name}</div>
                              <div className="text-xs text-base-content/50">{agent.role}</div>
                            </div>
                          </div>
                          <input
                            type="checkbox"
                            className="toggle toggle-primary toggle-sm"
                            checked={isSelected}
                            onChange={() => onHandleToggleCreateMember(agent.id)}
                          />
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
              <div className="modal-action">
                <button
                  className="btn btn-ghost btn-sm"
                  type="button"
                  onClick={() => onSetCreateModalOpen(false)}
                >
                  {t("cancel")}
                </button>
                <button className="btn btn-primary btn-sm" type="submit">
                  {t("createGroup")}
                </button>
              </div>
            </form>
          </div>
        </dialog>
      )}

      {editModalOpen && currentGroup && (
        <dialog className="modal modal-open" onClick={() => onSetEditModalOpen(false)}>
          <div className="modal-box" onClick={(event) => event.stopPropagation()}>
            <button
              className="btn btn-sm btn-circle btn-ghost absolute right-3 top-3"
              onClick={() => onSetEditModalOpen(false)}
            >
              ✕
            </button>
            <h3 className="mb-4 text-lg font-bold">
              {t("edit")} - {currentGroup.name}
            </h3>
            <form className="space-y-3" onSubmit={(event) => void onHandleUpdateGroup(event)}>
              <input
                className="input input-bordered input-sm w-full"
                placeholder={t("name")}
                required
                value={editGroupForm.name}
                onChange={(event: ChangeEvent<HTMLInputElement>) =>
                  onSetEditGroupForm((form) => ({ ...form, name: event.target.value }))
                }
              />
              <textarea
                className="textarea textarea-bordered textarea-sm w-full"
                placeholder={t("sharedGoal")}
                rows={2}
                value={editGroupForm.goal}
                onChange={(event: ChangeEvent<HTMLTextAreaElement>) =>
                  onSetEditGroupForm((form) => ({ ...form, goal: event.target.value }))
                }
              />
              <div className="join w-full">
                <input
                  className="input input-bordered input-sm join-item w-full"
                  placeholder={t("workingDirectory")}
                  required
                  value={editGroupForm.workingDirectory}
                  onChange={(event: ChangeEvent<HTMLInputElement>) =>
                    onSetEditGroupForm((form) => ({
                      ...form,
                      workingDirectory: event.target.value,
                    }))
                  }
                />
                <button
                  className="btn btn-outline btn-sm join-item"
                  type="button"
                  title={t("chooseFolder")}
                  aria-label={t("chooseFolder")}
                  onClick={() => void onHandlePickWorkingDirectory("edit")}
                >
                  <i className="fas fa-folder-open" />
                </button>
              </div>
              {directoryPickerError && (
                <div className="text-xs text-error">{directoryPickerError}</div>
              )}
              <select
                className="select select-bordered select-sm w-full"
                value={editGroupForm.kind}
                onChange={(event: ChangeEvent<HTMLSelectElement>) =>
                  onSetEditGroupForm((form) => ({
                    ...form,
                    kind: event.target.value as UpdateWorkGroupInput["kind"],
                  }))
                }
              >
                <option value="persistent">{t("persistent")}</option>
                <option value="ephemeral">{t("ephemeral")}</option>
              </select>
              <div className="modal-action">
                <button
                  className="btn btn-ghost btn-sm"
                  type="button"
                  onClick={() => onSetEditModalOpen(false)}
                >
                  {t("cancel")}
                </button>
                <button className="btn btn-primary btn-sm" type="submit">
                  {t("save")}
                </button>
              </div>
            </form>
          </div>
        </dialog>
      )}

      {deleteTargetGroup && (
        <dialog className="modal modal-open" onClick={() => onSetDeleteTargetGroup(null)}>
          <div className="modal-box" onClick={(event) => event.stopPropagation()}>
            <button
              className="btn btn-sm btn-circle btn-ghost absolute right-3 top-3"
              onClick={() => onSetDeleteTargetGroup(null)}
              disabled={deletingGroup}
            >
              ✕
            </button>
            <h3 className="mb-3 text-lg font-bold">{t("areYouSure")}</h3>
            <p className="text-sm text-base-content/70">
              {t("deleteWorkGroupConfirmHint", { name: deleteTargetGroup.name })}
            </p>
            <div className="modal-action">
              <button
                className="btn btn-ghost btn-sm"
                type="button"
                onClick={() => onSetDeleteTargetGroup(null)}
                disabled={deletingGroup}
              >
                {t("cancel")}
              </button>
              <button
                className="btn btn-error btn-sm"
                type="button"
                onClick={() => void onConfirmDeleteGroup()}
                disabled={deletingGroup}
              >
                {deletingGroup ? "..." : t("delete")}
              </button>
            </div>
          </div>
        </dialog>
      )}

      {clearHistoryTargetGroup && (
        <dialog className="modal modal-open" onClick={() => onSetClearHistoryTargetGroup(null)}>
          <div className="modal-box" onClick={(event) => event.stopPropagation()}>
            <button
              className="btn btn-sm btn-circle btn-ghost absolute right-3 top-3"
              onClick={() => onSetClearHistoryTargetGroup(null)}
              disabled={clearingHistory}
            >
              ✕
            </button>
            <h3 className="mb-3 text-lg font-bold">{t("clearHistory")}</h3>
            <p className="text-sm text-base-content/70">
              {t("clearHistoryConfirmHint", { name: clearHistoryTargetGroup.name })}
            </p>
            <div className="modal-action">
              <button
                className="btn btn-ghost btn-sm"
                type="button"
                onClick={() => onSetClearHistoryTargetGroup(null)}
                disabled={clearingHistory}
              >
                {t("cancel")}
              </button>
              <button
                className="btn btn-warning btn-sm"
                type="button"
                onClick={() => void onConfirmClearHistory()}
                disabled={clearingHistory}
              >
                {clearingHistory ? "..." : t("clearHistory")}
              </button>
            </div>
          </div>
        </dialog>
      )}
    </>
  );
}

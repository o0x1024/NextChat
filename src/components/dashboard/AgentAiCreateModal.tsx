import { type ChangeEvent } from "react";
import { useTranslation } from "react-i18next";

interface AgentAiCreateModalProps {
    open: boolean;
    prompt: string;
    error: string | null;
    loading: boolean;
    onClose: () => void;
    onChangePrompt: (prompt: string) => void;
    onSubmit: () => void;
}

export function AgentAiCreateModal({
    open,
    prompt,
    error,
    loading,
    onClose,
    onChangePrompt,
    onSubmit,
}: AgentAiCreateModalProps) {
    const { t } = useTranslation();

    if (!open) {
        return null;
    }

    return (
        <dialog className="modal modal-open bg-base-300/40 backdrop-blur-md" onClick={() => !loading && onClose()}>
            <div className="modal-box max-w-xl rounded-2xl border border-base-content/10 shadow-2xl" onClick={(event) => event.stopPropagation()}>
                <h3 className="text-lg font-bold">{t("aiCreateAgent")}</h3>
                <p className="mt-2 text-sm text-base-content/60">{t("aiCreateAgentPrompt")}</p>
                <textarea
                    rows={6}
                    className="textarea textarea-bordered mt-4 w-full bg-base-200/50"
                    placeholder={t("aiCreateAgentInputPlaceholder")}
                    value={prompt}
                    onChange={(event: ChangeEvent<HTMLTextAreaElement>) => onChangePrompt(event.target.value)}
                />
                {error && (
                    <div className="mt-3 rounded-lg border border-error/20 bg-error/10 px-3 py-2 text-xs text-error">
                        {error}
                    </div>
                )}
                <div className="modal-action">
                    <button className="btn btn-ghost" type="button" disabled={loading} onClick={onClose}>
                        {t("cancel")}
                    </button>
                    <button className="btn btn-secondary" type="button" disabled={loading} onClick={onSubmit}>
                        <i className={`fas ${loading ? "fa-spinner fa-spin" : "fa-wand-magic-sparkles"} text-xs`} />
                        {loading ? t("aiCreatingAgent") : t("aiCreateAgent")}
                    </button>
                </div>
            </div>
        </dialog>
    );
}

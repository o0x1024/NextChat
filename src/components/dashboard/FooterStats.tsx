import { useTranslation } from "react-i18next";

interface FooterStatsProps {
  auditEventsCount: number;
  toolsCount: number;
  skillsCount: number;
}

export function FooterStats({
  auditEventsCount,
  toolsCount,
  skillsCount,
}: FooterStatsProps) {
  const { t } = useTranslation();

  return (
    <footer className="stats stats-vertical w-full bg-base-200 md:stats-horizontal">
      <div className="stat">
        <div className="stat-title">{t("auditEvents")}</div>
        <div className="stat-value text-2xl">{auditEventsCount}</div>
      </div>
      <div className="stat">
        <div className="stat-title">{t("toolManifests")}</div>
        <div className="stat-value text-2xl">{toolsCount}</div>
      </div>
      <div className="stat">
        <div className="stat-title">{t("skills")}</div>
        <div className="stat-value text-2xl">{skillsCount}</div>
      </div>
    </footer>
  );
}

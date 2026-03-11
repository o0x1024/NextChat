const CHAIN_BADGES = ["badge-primary","badge-secondary","badge-accent","badge-info","badge-success"];

function suffix(value: string) {
  return value.replace(/[^a-zA-Z0-9]/g, "").slice(-4) || value.slice(-4);
}

export function chainBadgeClass(key: string) {
  const hash = [...key].reduce((sum, char) => sum + char.charCodeAt(0), 0);
  return CHAIN_BADGES[hash % CHAIN_BADGES.length];
}

export function narrativeChainToken(target: { taskId?: string | null; blockerId?: string | null; stageId?: string | null }) {
  if (target.blockerId) return { key: `blocker:${target.blockerId}`, label: `B-${suffix(target.blockerId)}` };
  if (target.taskId) return { key: `task:${target.taskId}`, label: `T-${suffix(target.taskId)}` };
  if (target.stageId) return { key: `stage:${target.stageId}`, label: `S-${suffix(target.stageId)}` };
  return null;
}

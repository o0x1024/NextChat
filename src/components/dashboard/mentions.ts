import type { AgentProfile } from "../../types";

export interface MentionDraft {
  start: number;
  end: number;
  query: string;
}

export interface MentionValidationResult {
  invalidMentions: string[];
  ambiguousMentions: string[];
}

const mentionRegex = /@([A-Za-z0-9_\-\u4e00-\u9fa5]+)/g;

export function activeMentionDraft(
  value: string,
  caret: number,
): MentionDraft | null {
  const beforeCaret = value.slice(0, caret);
  const match = /(?:^|\s)@([A-Za-z0-9_\-\u4e00-\u9fa5]*)$/.exec(beforeCaret);
  if (!match) {
    return null;
  }

  const query = match[1] ?? "";
  return {
    start: caret - query.length - 1,
    end: caret,
    query,
  };
}

export function mentionCandidates(
  agents: AgentProfile[],
  query: string,
): AgentProfile[] {
  const normalized = query.trim().toLowerCase();
  if (!normalized) {
    return agents;
  }

  return agents.filter((agent) => {
    const haystack = [
      agent.name,
      agent.id,
      agent.role,
      agent.objective,
    ].join(" ").toLowerCase();
    return haystack.includes(normalized);
  });
}

export function insertMention(
  value: string,
  draft: MentionDraft,
  agent: AgentProfile,
): { nextValue: string; caret: number } {
  const inserted = `@${agent.name} `;
  const nextValue = `${value.slice(0, draft.start)}${inserted}${value.slice(draft.end)}`;
  return {
    nextValue,
    caret: draft.start + inserted.length,
  };
}

export function validateMentions(
  value: string,
  agents: AgentProfile[],
): MentionValidationResult {
  const grouped = agents.reduce<Map<string, AgentProfile[]>>((acc, agent) => {
    const key = agent.name.toLowerCase();
    const existing = acc.get(key) ?? [];
    existing.push(agent);
    acc.set(key, existing);
    return acc;
  }, new Map());

  const invalidMentions = new Set<string>();
  const ambiguousMentions = new Set<string>();

  for (const match of value.matchAll(mentionRegex)) {
    const token = match[1]?.toLowerCase();
    if (!token) {
      continue;
    }
    const candidates = grouped.get(token);
    if (!candidates || candidates.length === 0) {
      invalidMentions.add(token);
    } else if (candidates.length > 1) {
      ambiguousMentions.add(token);
    }
  }

  return {
    invalidMentions: Array.from(invalidMentions),
    ambiguousMentions: Array.from(ambiguousMentions),
  };
}

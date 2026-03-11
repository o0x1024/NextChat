import type { ConversationMessage } from "../../types";
import { parseNarrativeContent } from "./narrative";

export function findLatestNarrativeMessageId(
  messages: ConversationMessage[],
  target: { taskId?: string; blockerId?: string },
) {
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    const narrative = parseNarrativeContent(message.content, message);
    if (!narrative) continue;
    if (target.blockerId && narrative.blockerId === target.blockerId) {
      return message.id;
    }
    if (target.taskId && narrative.taskId === target.taskId) {
      return message.id;
    }
  }
  return null;
}

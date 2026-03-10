use anyhow::Result;
use tauri::{AppHandle, Runtime};

use super::{emit, AppService};
use crate::core::domain::{now, ChatStreamEvent, ChatStreamPhase, ConversationMessage};

impl AppService {
    pub(super) fn emit_stream_events<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        message: &ConversationMessage,
    ) -> Result<()> {
        let mut sequence: i64 = 0;
        let start = ChatStreamEvent {
            stream_id: message.id.clone(),
            phase: ChatStreamPhase::Start,
            conversation_id: message.conversation_id.clone(),
            work_group_id: message.work_group_id.clone(),
            sender_id: message.sender_id.clone(),
            sender_name: message.sender_name.clone(),
            kind: message.kind.clone(),
            visibility: message.visibility.clone(),
            task_card_id: message.task_card_id.clone(),
            sequence,
            delta: None,
            full_content: None,
            created_at: now(),
        };
        emit(app, "chat:stream-start", &start)?;

        for chunk in stream_chunks(&message.content, 48) {
            sequence += 1;
            let delta = ChatStreamEvent {
                stream_id: message.id.clone(),
                phase: ChatStreamPhase::Delta,
                conversation_id: message.conversation_id.clone(),
                work_group_id: message.work_group_id.clone(),
                sender_id: message.sender_id.clone(),
                sender_name: message.sender_name.clone(),
                kind: message.kind.clone(),
                visibility: message.visibility.clone(),
                task_card_id: message.task_card_id.clone(),
                sequence,
                delta: Some(chunk),
                full_content: None,
                created_at: now(),
            };
            emit(app, "chat:stream-delta", &delta)?;
        }

        sequence += 1;
        let done = ChatStreamEvent {
            stream_id: message.id.clone(),
            phase: ChatStreamPhase::Done,
            conversation_id: message.conversation_id.clone(),
            work_group_id: message.work_group_id.clone(),
            sender_id: message.sender_id.clone(),
            sender_name: message.sender_name.clone(),
            kind: message.kind.clone(),
            visibility: message.visibility.clone(),
            task_card_id: message.task_card_id.clone(),
            sequence,
            delta: None,
            full_content: Some(message.content.clone()),
            created_at: now(),
        };
        emit(app, "chat:stream-done", &done)?;
        Ok(())
    }
}

fn stream_chunks(content: &str, max_chunk_chars: usize) -> Vec<String> {
    if content.is_empty() || max_chunk_chars == 0 {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut count = 0usize;
    for ch in content.chars() {
        current.push(ch);
        count += 1;
        if count >= max_chunk_chars || ch == '\n' {
            chunks.push(std::mem::take(&mut current));
            count = 0;
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

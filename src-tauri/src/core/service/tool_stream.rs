use std::collections::HashMap;

use anyhow::Result;
use tauri::{AppHandle, Runtime};

use super::emit;
use crate::core::domain::{
    new_id, now, ChatStreamEvent, ChatStreamPhase, MessageKind, ToolStreamChunk, Visibility,
};

struct ToolTrack {
    stream_id: String,
    sender_name: String,
    sequence: i64,
    full_content: String,
}

pub(super) struct ToolStreamSession {
    conversation_id: String,
    work_group_id: String,
    sender_id: String,
    sender_name: String,
    task_card_id: Option<String>,
    tracks: HashMap<String, ToolTrack>,
}

impl ToolStreamSession {
    pub(super) fn new(
        conversation_id: String,
        work_group_id: String,
        sender_id: String,
        sender_name: String,
        task_card_id: Option<String>,
    ) -> Self {
        Self {
            conversation_id,
            work_group_id,
            sender_id,
            sender_name,
            task_card_id,
            tracks: HashMap::new(),
        }
    }

    pub(super) fn handle_chunk<R: Runtime>(
        &mut self,
        app: &AppHandle<R>,
        chunk: ToolStreamChunk,
    ) -> Result<()> {
        if chunk.delta.is_empty() {
            return Ok(());
        }

        let track = self.tracks.entry(chunk.tool_id.clone()).or_insert_with(|| {
            let stream_id = new_id();
            let sender_name = format!("{} · {}", self.sender_name, chunk.tool_id);
            let start_event = ChatStreamEvent {
                stream_id: stream_id.clone(),
                phase: ChatStreamPhase::Start,
                conversation_id: self.conversation_id.clone(),
                work_group_id: self.work_group_id.clone(),
                sender_id: self.sender_id.clone(),
                sender_name: sender_name.clone(),
                kind: MessageKind::ToolResult,
                visibility: Visibility::Backstage,
                task_card_id: self.task_card_id.clone(),
                sequence: 0,
                delta: None,
                full_content: None,
                created_at: now(),
            };
            let _ = emit(app, "chat:stream-start", &start_event);
            ToolTrack {
                stream_id,
                sender_name,
                sequence: 0,
                full_content: String::new(),
            }
        });

        track.sequence += 1;
        let delta = if chunk.channel == "stderr" {
            format!("[stderr] {}", chunk.delta)
        } else {
            chunk.delta
        };
        track.full_content.push_str(&delta);

        let event = ChatStreamEvent {
            stream_id: track.stream_id.clone(),
            phase: ChatStreamPhase::Delta,
            conversation_id: self.conversation_id.clone(),
            work_group_id: self.work_group_id.clone(),
            sender_id: self.sender_id.clone(),
            sender_name: track.sender_name.clone(),
            kind: MessageKind::ToolResult,
            visibility: Visibility::Backstage,
            task_card_id: self.task_card_id.clone(),
            sequence: track.sequence,
            delta: Some(delta),
            full_content: None,
            created_at: now(),
        };
        emit(app, "chat:stream-delta", &event)?;
        Ok(())
    }

    pub(super) fn finish<R: Runtime>(mut self, app: &AppHandle<R>) -> Result<usize> {
        let count = self.tracks.len();
        for track in self.tracks.values_mut() {
            track.sequence += 1;
            let done_event = ChatStreamEvent {
                stream_id: track.stream_id.clone(),
                phase: ChatStreamPhase::Done,
                conversation_id: self.conversation_id.clone(),
                work_group_id: self.work_group_id.clone(),
                sender_id: self.sender_id.clone(),
                sender_name: track.sender_name.clone(),
                kind: MessageKind::ToolResult,
                visibility: Visibility::Backstage,
                task_card_id: self.task_card_id.clone(),
                sequence: track.sequence,
                delta: None,
                full_content: Some(track.full_content.clone()),
                created_at: now(),
            };
            emit(app, "chat:stream-done", &done_event)?;
        }
        Ok(count)
    }
}

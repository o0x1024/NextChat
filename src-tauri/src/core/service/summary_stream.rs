use anyhow::Result;
use tauri::{AppHandle, Runtime};

use super::emit;
use crate::core::domain::{
    new_id, now, ChatStreamEvent, ChatStreamPhase, MessageKind, SummaryStreamSignal, Visibility,
};

#[derive(Debug, Clone)]
pub(super) struct SummaryCommittedSegment {
    pub(super) stream_id: String,
    pub(super) content: String,
    pub(super) started_at: String,
}

#[derive(Debug, Clone)]
pub(super) struct SummaryStreamSnapshot {
    pub(super) committed_segments: Vec<SummaryCommittedSegment>,
    pub(super) current_stream_id: String,
    pub(super) current_content: String,
    pub(super) current_sequence: i64,
    pub(super) current_started: bool,
}

pub(super) struct SummaryStreamSession {
    conversation_id: String,
    work_group_id: String,
    sender_id: String,
    sender_name: String,
    task_card_id: Option<String>,
    current_stream_id: String,
    current_content: String,
    current_started_at: String,
    current_sequence: i64,
    current_started: bool,
    committed_segments: Vec<SummaryCommittedSegment>,
}

impl SummaryStreamSession {
    pub(super) fn new(
        stream_id: String,
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
            current_stream_id: stream_id,
            current_content: String::new(),
            current_started_at: now(),
            current_sequence: 0,
            current_started: false,
            committed_segments: Vec::new(),
        }
    }

    pub(super) fn start_current<R: Runtime>(&mut self, app: &AppHandle<R>) -> Result<()> {
        if self.current_started {
            return Ok(());
        }
        let event = self.build_event(
            ChatStreamPhase::Start,
            0,
            None,
            None,
            self.current_started_at.clone(),
        );
        emit(app, "chat:stream-start", &event)?;
        self.current_started = true;
        Ok(())
    }

    pub(super) fn handle_signal<R: Runtime>(
        &mut self,
        app: &AppHandle<R>,
        signal: SummaryStreamSignal,
    ) -> Result<()> {
        match signal {
            SummaryStreamSignal::Delta(delta) => {
                if delta.is_empty() {
                    return Ok(());
                }
                self.start_current(app)?;
                self.current_sequence += 1;
                self.current_content.push_str(&delta);
                let event = self.build_event(
                    ChatStreamPhase::Delta,
                    self.current_sequence,
                    Some(delta),
                    None,
                    now(),
                );
                emit(app, "chat:stream-delta", &event)?;
            }
            SummaryStreamSignal::Reset => {
                self.commit_current_segment(app)?;
            }
        }
        Ok(())
    }

    fn commit_current_segment<R: Runtime>(&mut self, app: &AppHandle<R>) -> Result<()> {
        if self.current_started && !self.current_content.is_empty() {
            self.current_sequence += 1;
            let event = self.build_event(
                ChatStreamPhase::Done,
                self.current_sequence,
                None,
                Some(self.current_content.clone()),
                now(),
            );
            emit(app, "chat:stream-done", &event)?;
            self.committed_segments.push(SummaryCommittedSegment {
                stream_id: self.current_stream_id.clone(),
                content: self.current_content.clone(),
                started_at: self.current_started_at.clone(),
            });
        }

        self.current_stream_id = new_id();
        self.current_content.clear();
        self.current_started_at = now();
        self.current_sequence = 0;
        self.current_started = false;
        Ok(())
    }

    pub(super) fn into_snapshot(self) -> SummaryStreamSnapshot {
        SummaryStreamSnapshot {
            committed_segments: self.committed_segments,
            current_stream_id: self.current_stream_id,
            current_content: self.current_content,
            current_sequence: self.current_sequence,
            current_started: self.current_started,
        }
    }

    fn build_event(
        &self,
        phase: ChatStreamPhase,
        sequence: i64,
        delta: Option<String>,
        full_content: Option<String>,
        created_at: String,
    ) -> ChatStreamEvent {
        ChatStreamEvent {
            stream_id: self.current_stream_id.clone(),
            phase,
            conversation_id: self.conversation_id.clone(),
            work_group_id: self.work_group_id.clone(),
            sender_id: self.sender_id.clone(),
            sender_name: self.sender_name.clone(),
            kind: MessageKind::Summary,
            visibility: Visibility::Main,
            task_card_id: self.task_card_id.clone(),
            sequence,
            delta,
            full_content,
            created_at,
        }
    }
}

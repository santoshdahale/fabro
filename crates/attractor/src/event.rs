use serde::{Deserialize, Serialize};

use crate::outcome::StageUsage;

/// Events emitted during pipeline execution for observability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PipelineEvent {
    PipelineStarted {
        name: String,
        id: String,
    },
    PipelineCompleted {
        duration_ms: u64,
        artifact_count: usize,
    },
    PipelineFailed {
        error: String,
        duration_ms: u64,
    },
    StageStarted {
        name: String,
        index: usize,
    },
    StageCompleted {
        name: String,
        index: usize,
        duration_ms: u64,
        status: String,
        preferred_label: Option<String>,
        suggested_next_ids: Vec<String>,
        usage: Option<StageUsage>,
    },
    StageFailed {
        name: String,
        index: usize,
        error: String,
        will_retry: bool,
    },
    StageRetrying {
        name: String,
        index: usize,
        attempt: usize,
        delay_ms: u64,
    },
    ParallelStarted {
        branch_count: usize,
    },
    ParallelBranchStarted {
        branch: String,
        index: usize,
    },
    ParallelBranchCompleted {
        branch: String,
        index: usize,
        duration_ms: u64,
        success: bool,
    },
    ParallelCompleted {
        duration_ms: u64,
        success_count: usize,
        failure_count: usize,
    },
    InterviewStarted {
        question: String,
        stage: String,
    },
    InterviewCompleted {
        question: String,
        answer: String,
        duration_ms: u64,
    },
    InterviewTimeout {
        question: String,
        stage: String,
        duration_ms: u64,
    },
    CheckpointSaved {
        node_id: String,
    },
    Prompt {
        stage: String,
        text: String,
    },
    AssistantMessage {
        stage: String,
        text: String,
        model: String,
        input_tokens: i64,
        output_tokens: i64,
        tool_call_count: usize,
    },
    ToolCallStarted {
        stage: String,
        tool_name: String,
        tool_call_id: String,
        arguments: serde_json::Value,
    },
    ToolCallCompleted {
        stage: String,
        tool_name: String,
        tool_call_id: String,
        output: serde_json::Value,
        is_error: bool,
    },
    SessionError {
        stage: String,
        error: String,
    },
    ContextWindowWarning {
        stage: String,
        estimated_tokens: usize,
        context_window_size: usize,
        usage_percent: usize,
    },
    LoopDetected {
        stage: String,
    },
    TurnLimitReached {
        stage: String,
    },
    CompactionStarted {
        stage: String,
        estimated_tokens: usize,
        context_window_size: usize,
    },
    CompactionCompleted {
        stage: String,
        original_turn_count: usize,
        preserved_turn_count: usize,
        summary_token_estimate: usize,
    },
}

/// Listener callback type for pipeline events.
type EventListener = Box<dyn Fn(&PipelineEvent) + Send + Sync>;

/// Callback-based event emitter for pipeline events.
pub struct EventEmitter {
    listeners: Vec<EventListener>,
}

impl std::fmt::Debug for EventEmitter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventEmitter")
            .field("listener_count", &self.listeners.len())
            .finish()
    }
}

impl Default for EventEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl EventEmitter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            listeners: Vec::new(),
        }
    }

    pub fn on_event(&mut self, listener: impl Fn(&PipelineEvent) + Send + Sync + 'static) {
        self.listeners.push(Box::new(listener));
    }

    pub fn emit(&self, event: &PipelineEvent) {
        for listener in &self.listeners {
            listener(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn event_emitter_new_has_no_listeners() {
        let emitter = EventEmitter::new();
        assert_eq!(emitter.listeners.len(), 0);
    }

    #[test]
    fn event_emitter_calls_listener() {
        let mut emitter = EventEmitter::new();
        let received = Arc::new(Mutex::new(Vec::new()));
        let received_clone = Arc::clone(&received);
        emitter.on_event(move |event| {
            let name = match event {
                PipelineEvent::PipelineStarted { name, .. } => name.clone(),
                _ => "other".to_string(),
            };
            received_clone.lock().unwrap().push(name);
        });
        emitter.emit(&PipelineEvent::PipelineStarted {
            name: "test".to_string(),
            id: "1".to_string(),
        });
        let events = received.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], "test");
    }

    #[test]
    fn pipeline_event_serialization() {
        let event = PipelineEvent::StageStarted {
            name: "plan".to_string(),
            index: 0,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("StageStarted"));
        assert!(json.contains("plan"));
    }

    #[test]
    fn event_emitter_default() {
        let emitter = EventEmitter::default();
        assert_eq!(emitter.listeners.len(), 0);
    }

    #[test]
    fn llm_conversation_event_serialization() {
        let event = PipelineEvent::ToolCallStarted {
            stage: "plan".to_string(),
            tool_name: "read_file".to_string(),
            tool_call_id: "call_1".to_string(),
            arguments: serde_json::json!({"path": "/tmp/test.txt"}),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("ToolCallStarted"));
        assert!(json.contains("read_file"));
        assert!(json.contains("plan"));

        // Verify round-trip
        let deserialized: PipelineEvent = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, PipelineEvent::ToolCallStarted { stage, .. } if stage == "plan"));
    }

    #[test]
    fn assistant_message_event_serialization() {
        let event = PipelineEvent::AssistantMessage {
            stage: "code".to_string(),
            text: "Here is the implementation".to_string(),
            model: "claude-opus-4-6".to_string(),
            input_tokens: 1000,
            output_tokens: 500,
            tool_call_count: 3,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("AssistantMessage"));
        assert!(json.contains("claude-opus-4-6"));
    }

    #[test]
    fn compaction_pipeline_event_serialization() {
        let started = PipelineEvent::CompactionStarted {
            stage: "code".to_string(),
            estimated_tokens: 5000,
            context_window_size: 8000,
        };
        let json = serde_json::to_string(&started).unwrap();
        assert!(json.contains("CompactionStarted"));
        let deserialized: PipelineEvent = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, PipelineEvent::CompactionStarted { stage, .. } if stage == "code"));

        let completed = PipelineEvent::CompactionCompleted {
            stage: "code".to_string(),
            original_turn_count: 20,
            preserved_turn_count: 6,
            summary_token_estimate: 500,
        };
        let json = serde_json::to_string(&completed).unwrap();
        assert!(json.contains("CompactionCompleted"));
        let deserialized: PipelineEvent = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, PipelineEvent::CompactionCompleted { stage, .. } if stage == "code"));
    }
}

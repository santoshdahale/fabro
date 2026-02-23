use std::path::Path;

use async_trait::async_trait;

use crate::context::Context;
use crate::error::AttractorError;
use crate::graph::{Graph, Node};
use crate::outcome::Outcome;

use super::{EngineServices, Handler};

/// Conditional routing handler. Returns SUCCESS with a note; actual routing
/// is handled by the engine's edge selection algorithm.
pub struct ConditionalHandler;

#[async_trait]
impl Handler for ConditionalHandler {
    async fn execute(
        &self,
        node: &Node,
        _context: &Context,
        _graph: &Graph,
        _logs_root: &Path,
        _services: &EngineServices,
    ) -> Result<Outcome, AttractorError> {
        let mut outcome = Outcome::success();
        outcome.notes = Some(format!("Conditional node evaluated: {}", node.id));
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventEmitter;
    use crate::handler::start::StartHandler;
    use crate::handler::HandlerRegistry;

    fn make_services() -> EngineServices {
        EngineServices {
            registry: std::sync::Arc::new(HandlerRegistry::new(Box::new(StartHandler))),
            emitter: std::sync::Arc::new(EventEmitter::new()),
        }
    }

    #[tokio::test]
    async fn conditional_handler_returns_success_with_note() {
        let handler = ConditionalHandler;
        let node = Node::new("gate");
        let context = Context::new();
        let graph = Graph::new("test");
        let logs_root = Path::new("/tmp/test");
        let outcome = handler
            .execute(&node, &context, &graph, logs_root, &make_services())
            .await
            .unwrap();
        assert_eq!(outcome.status, crate::outcome::StageStatus::Success);
        assert_eq!(
            outcome.notes.as_deref(),
            Some("Conditional node evaluated: gate")
        );
    }
}

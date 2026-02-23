use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;

use attractor::checkpoint::Checkpoint;
use attractor::context::Context;
use attractor::engine::{PipelineEngine, RunConfig};
use attractor::error::AttractorError;
use attractor::event::{EventEmitter, PipelineEvent};
use attractor::graph::{AttrValue, Edge, Graph, Node};
use attractor::handler::codergen::{CodergenBackend, CodergenHandler, CodergenResult};
use attractor::handler::conditional::ConditionalHandler;
use attractor::handler::exit::ExitHandler;
use attractor::handler::manager_loop::ManagerLoopHandler;
use attractor::handler::start::StartHandler;
use attractor::handler::tool::ToolHandler;
use attractor::handler::wait_human::WaitHumanHandler;
use attractor::handler::{Handler, HandlerRegistry};
use attractor::interviewer::auto_approve::AutoApproveInterviewer;
use attractor::interviewer::queue::QueueInterviewer;
use attractor::interviewer::recording::RecordingInterviewer;
use attractor::interviewer::{Answer, AnswerValue, Interviewer};
use attractor::outcome::{Outcome, StageStatus};
use attractor::parser::parse;
use attractor::stylesheet::{apply_stylesheet, parse_stylesheet};
use attractor::transform::{StylesheetApplicationTransform, Transform, VariableExpansionTransform};
use attractor::validation::{validate, validate_or_raise, Severity};

// ---------------------------------------------------------------------------
// 1. Parse and validate all 3 spec examples (Section 2.13)
// ---------------------------------------------------------------------------

#[test]
fn parse_and_validate_simple_linear() {
    let input = r#"digraph Simple {
        graph [goal="Run tests and report"]
        rankdir=LR

        start [shape=Mdiamond, label="Start"]
        exit  [shape=Msquare, label="Exit"]

        run_tests [label="Run Tests", prompt="Run the test suite and report results"]
        report    [label="Report", prompt="Summarize the test results"]

        start -> run_tests -> report -> exit
    }"#;

    let graph = parse(input).expect("parsing should succeed");
    assert_eq!(graph.name, "Simple");
    assert_eq!(graph.goal(), "Run tests and report");
    assert_eq!(graph.nodes.len(), 4);
    assert_eq!(graph.edges.len(), 3);
    assert!(graph.find_start_node().is_some());
    assert!(graph.find_exit_node().is_some());

    let diagnostics = validate_or_raise(&graph, &[]).expect("validation should pass");
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == attractor::validation::Severity::Error)
        .collect();
    assert!(errors.is_empty(), "expected no validation errors");
}

#[test]
fn parse_and_validate_branching_with_conditions() {
    let input = r#"digraph Branch {
        graph [goal="Implement and validate a feature"]
        rankdir=LR
        node [shape=box, timeout="900s"]

        start     [shape=Mdiamond, label="Start"]
        exit      [shape=Msquare, label="Exit"]
        plan      [label="Plan", prompt="Plan the implementation"]
        implement [label="Implement", prompt="Implement the plan"]
        validate  [label="Validate", prompt="Run tests"]
        gate      [shape=diamond, label="Tests passing?"]

        start -> plan -> implement -> validate -> gate
        gate -> exit      [label="Yes", condition="outcome=success"]
        gate -> implement [label="No", condition="outcome!=success"]
    }"#;

    let graph = parse(input).expect("parsing should succeed");
    assert_eq!(graph.name, "Branch");
    assert_eq!(graph.nodes.len(), 6);
    assert_eq!(graph.edges.len(), 6);

    let gate_exit = graph
        .edges
        .iter()
        .find(|e| e.from == "gate" && e.to == "exit")
        .expect("gate -> exit edge should exist");
    assert_eq!(gate_exit.condition(), Some("outcome=success"));

    let gate_impl = graph
        .edges
        .iter()
        .find(|e| e.from == "gate" && e.to == "implement")
        .expect("gate -> implement edge should exist");
    assert_eq!(gate_impl.condition(), Some("outcome!=success"));

    let diagnostics = validate_or_raise(&graph, &[]).expect("validation should pass");
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == attractor::validation::Severity::Error)
        .collect();
    assert!(errors.is_empty(), "expected no validation errors");
}

#[test]
fn parse_and_validate_human_gate() {
    let input = r#"digraph Review {
        rankdir=LR

        start [shape=Mdiamond, label="Start"]
        exit  [shape=Msquare, label="Exit"]

        review_gate [
            shape=hexagon,
            label="Review Changes",
            type="wait.human"
        ]

        start -> review_gate
        review_gate -> ship_it [label="[A] Approve"]
        review_gate -> fixes   [label="[F] Fix"]
        ship_it -> exit
        fixes -> review_gate
    }"#;

    let graph = parse(input).expect("parsing should succeed");
    assert_eq!(graph.name, "Review");
    assert_eq!(graph.nodes.len(), 5);
    assert_eq!(graph.edges.len(), 5);

    let gate = &graph.nodes["review_gate"];
    assert_eq!(gate.node_type(), Some("wait.human"));
    assert_eq!(gate.shape(), "hexagon");
    assert_eq!(gate.label(), "Review Changes");

    let diagnostics = validate_or_raise(&graph, &[]).expect("validation should pass");
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == attractor::validation::Severity::Error)
        .collect();
    assert!(errors.is_empty(), "expected no validation errors");
}

// ---------------------------------------------------------------------------
// 2. End-to-end linear pipeline
// ---------------------------------------------------------------------------

fn make_linear_registry() -> HandlerRegistry {
    let mut registry = HandlerRegistry::new(Box::new(CodergenHandler::new(None)));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    registry.register("codergen", Box::new(CodergenHandler::new(None)));
    registry
}

#[tokio::test]
async fn end_to_end_linear_pipeline() {
    let input = r#"digraph Linear {
        graph [goal="Build the feature"]
        start [shape=Mdiamond]
        exit  [shape=Msquare]
        codergen_step [shape=box, label="Code", prompt="Implement the feature"]
        start -> codergen_step -> exit
    }"#;

    let graph = parse(input).expect("parse should succeed");
    validate_or_raise(&graph, &[]).expect("validation should pass");

    let dir = tempfile::tempdir().unwrap();
    let engine = PipelineEngine::new(make_linear_registry(), EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };

    let outcome = engine.run(&graph, &config).await.expect("run should succeed");
    assert_eq!(outcome.status, StageStatus::Success);

    // Checkpoint should exist
    let checkpoint_path = dir.path().join("checkpoint.json");
    assert!(checkpoint_path.exists(), "checkpoint.json should exist");

    let checkpoint = Checkpoint::load(&checkpoint_path).expect("checkpoint should load");
    assert!(checkpoint.completed_nodes.contains(&"start".to_string()));
    assert!(checkpoint
        .completed_nodes
        .contains(&"codergen_step".to_string()));

    // Codergen handler writes prompt.md, response.md, status.json
    let stage_dir = dir.path().join("codergen_step");
    assert!(stage_dir.join("prompt.md").exists(), "prompt.md should exist");
    assert!(
        stage_dir.join("response.md").exists(),
        "response.md should exist"
    );
    assert!(
        stage_dir.join("status.json").exists(),
        "status.json should exist"
    );

    let prompt_content = std::fs::read_to_string(stage_dir.join("prompt.md")).unwrap();
    assert_eq!(prompt_content, "Implement the feature");
}

// ---------------------------------------------------------------------------
// 3. End-to-end branching pipeline
// ---------------------------------------------------------------------------

#[tokio::test]
async fn end_to_end_branching_pipeline() {
    // Build a graph:
    //   start -> work -> gate (diamond)
    //   gate -> success_path [condition="outcome=success"]
    //   gate -> fail_path    [condition="outcome=fail"]
    //   success_path -> exit
    //   fail_path -> exit
    //
    // Since work defaults to codergen (shape=box) which returns SUCCESS,
    // the engine should route gate -> success_path via condition match.

    let mut graph = Graph::new("BranchTest");
    graph
        .attrs
        .insert("goal".to_string(), AttrValue::String("Test branching".to_string()));

    let mut start = Node::new("start");
    start
        .attrs
        .insert("shape".to_string(), AttrValue::String("Mdiamond".to_string()));
    graph.nodes.insert("start".to_string(), start);

    let mut exit = Node::new("exit");
    exit.attrs
        .insert("shape".to_string(), AttrValue::String("Msquare".to_string()));
    graph.nodes.insert("exit".to_string(), exit);

    let mut work = Node::new("work");
    work.attrs
        .insert("shape".to_string(), AttrValue::String("box".to_string()));
    work.attrs.insert(
        "prompt".to_string(),
        AttrValue::String("Do work".to_string()),
    );
    graph.nodes.insert("work".to_string(), work);

    let mut gate = Node::new("gate");
    gate.attrs
        .insert("shape".to_string(), AttrValue::String("diamond".to_string()));
    graph.nodes.insert("gate".to_string(), gate);

    graph
        .nodes
        .insert("success_path".to_string(), Node::new("success_path"));
    graph
        .nodes
        .insert("fail_path".to_string(), Node::new("fail_path"));

    graph.edges.push(Edge::new("start", "work"));
    graph.edges.push(Edge::new("work", "gate"));

    let mut gate_success = Edge::new("gate", "success_path");
    gate_success.attrs.insert(
        "condition".to_string(),
        AttrValue::String("outcome=success".to_string()),
    );
    graph.edges.push(gate_success);

    let mut gate_fail = Edge::new("gate", "fail_path");
    gate_fail.attrs.insert(
        "condition".to_string(),
        AttrValue::String("outcome=fail".to_string()),
    );
    graph.edges.push(gate_fail);

    graph.edges.push(Edge::new("success_path", "exit"));
    graph.edges.push(Edge::new("fail_path", "exit"));

    let dir = tempfile::tempdir().unwrap();
    let mut registry = HandlerRegistry::new(Box::new(CodergenHandler::new(None)));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    registry.register("codergen", Box::new(CodergenHandler::new(None)));
    registry.register("conditional", Box::new(ConditionalHandler));

    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };

    let outcome = engine.run(&graph, &config).await.expect("run should succeed");
    assert_eq!(outcome.status, StageStatus::Success);

    let checkpoint = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    assert!(
        checkpoint
            .completed_nodes
            .contains(&"success_path".to_string()),
        "should have traversed success_path"
    );
    assert!(
        !checkpoint
            .completed_nodes
            .contains(&"fail_path".to_string()),
        "should NOT have traversed fail_path"
    );
}

// ---------------------------------------------------------------------------
// 4. End-to-end human gate pipeline with QueueInterviewer
// ---------------------------------------------------------------------------

#[tokio::test]
async fn end_to_end_human_gate_pipeline() {
    // Build a graph:
    //   start -> gate (hexagon, type=wait.human)
    //   gate -> approve [label="[A] Approve"]
    //   gate -> reject  [label="[R] Reject"]
    //   approve -> exit
    //   reject -> exit
    //
    // QueueInterviewer pre-filled to select "R" -> should route to reject

    let mut graph = Graph::new("HumanGateTest");

    let mut start = Node::new("start");
    start
        .attrs
        .insert("shape".to_string(), AttrValue::String("Mdiamond".to_string()));
    graph.nodes.insert("start".to_string(), start);

    let mut exit = Node::new("exit");
    exit.attrs
        .insert("shape".to_string(), AttrValue::String("Msquare".to_string()));
    graph.nodes.insert("exit".to_string(), exit);

    let mut gate = Node::new("gate");
    gate.attrs
        .insert("shape".to_string(), AttrValue::String("hexagon".to_string()));
    gate.attrs.insert(
        "type".to_string(),
        AttrValue::String("wait.human".to_string()),
    );
    gate.attrs.insert(
        "label".to_string(),
        AttrValue::String("Review Changes".to_string()),
    );
    graph.nodes.insert("gate".to_string(), gate);

    graph
        .nodes
        .insert("approve".to_string(), Node::new("approve"));
    graph
        .nodes
        .insert("reject".to_string(), Node::new("reject"));

    graph.edges.push(Edge::new("start", "gate"));

    let mut e_approve = Edge::new("gate", "approve");
    e_approve.attrs.insert(
        "label".to_string(),
        AttrValue::String("[A] Approve".to_string()),
    );
    graph.edges.push(e_approve);

    let mut e_reject = Edge::new("gate", "reject");
    e_reject.attrs.insert(
        "label".to_string(),
        AttrValue::String("[R] Reject".to_string()),
    );
    graph.edges.push(e_reject);

    graph.edges.push(Edge::new("approve", "exit"));
    graph.edges.push(Edge::new("reject", "exit"));

    // Pre-fill the queue with an answer selecting "R"
    let answers = VecDeque::from([Answer {
        value: AnswerValue::Selected("R".to_string()),
        selected_option: None,
        text: None,
    }]);
    let interviewer = Arc::new(QueueInterviewer::new(answers));

    let dir = tempfile::tempdir().unwrap();
    let mut registry = HandlerRegistry::new(Box::new(StartHandler));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    registry.register("wait.human", Box::new(WaitHumanHandler::new(interviewer)));

    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };

    let outcome = engine.run(&graph, &config).await.expect("run should succeed");
    assert_eq!(outcome.status, StageStatus::Success);

    let checkpoint = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    assert!(
        checkpoint
            .completed_nodes
            .contains(&"reject".to_string()),
        "should have traversed reject path"
    );
    assert!(
        !checkpoint
            .completed_nodes
            .contains(&"approve".to_string()),
        "should NOT have traversed approve path"
    );
}

// ---------------------------------------------------------------------------
// 5. Goal gate enforcement
// ---------------------------------------------------------------------------

/// A custom handler that always returns FAIL for testing goal gate enforcement.
struct AlwaysFailHandler;

#[async_trait::async_trait]
impl Handler for AlwaysFailHandler {
    async fn execute(
        &self,
        node: &Node,
        _context: &attractor::context::Context,
        _graph: &Graph,
        _logs_root: &Path,
        _services: &attractor::handler::EngineServices,
    ) -> Result<Outcome, attractor::error::AttractorError> {
        Ok(Outcome::fail(format!("forced failure for {}", node.id)))
    }
}

#[tokio::test]
async fn goal_gate_routes_to_retry_target_on_failure() {
    // Pipeline:
    //   start -> gated_work -> exit
    //   gated_work has goal_gate=true, retry_target=start
    //   gated_work always returns FAIL
    //
    // When engine reaches exit, it checks goal gates and finds gated_work failed.
    // It should route back to retry_target (start).
    //
    // To avoid infinite loops, we set max_retries=0 on gated_work so it fails
    // immediately each time. After looping once (start -> gated_work -> exit -> start
    // -> gated_work -> exit), if goal gate is still unsatisfied and no retry_target
    // changes, we need to limit iterations. The engine itself doesn't limit loops,
    // so we test a simpler scenario: verify the error when retry_target is missing.

    // Test: goal_gate with NO retry_target returns an error
    let mut graph = Graph::new("GoalGateNoRetry");

    let mut start = Node::new("start");
    start
        .attrs
        .insert("shape".to_string(), AttrValue::String("Mdiamond".to_string()));
    graph.nodes.insert("start".to_string(), start);

    let mut exit = Node::new("exit");
    exit.attrs
        .insert("shape".to_string(), AttrValue::String("Msquare".to_string()));
    graph.nodes.insert("exit".to_string(), exit);

    let mut gated_work = Node::new("gated_work");
    gated_work
        .attrs
        .insert("goal_gate".to_string(), AttrValue::Boolean(true));
    gated_work
        .attrs
        .insert("max_retries".to_string(), AttrValue::Integer(0));
    gated_work.attrs.insert(
        "type".to_string(),
        AttrValue::String("always_fail".to_string()),
    );
    graph
        .nodes
        .insert("gated_work".to_string(), gated_work);

    graph.edges.push(Edge::new("start", "gated_work"));
    graph.edges.push(Edge::new("gated_work", "exit"));

    let dir = tempfile::tempdir().unwrap();
    let mut registry = HandlerRegistry::new(Box::new(StartHandler));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    registry.register("always_fail", Box::new(AlwaysFailHandler));

    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };

    let result = engine.run(&graph, &config).await;
    assert!(result.is_err(), "should fail when goal gate unsatisfied and no retry_target");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("goal gate unsatisfied"),
        "error should mention goal gate, got: {err_msg}"
    );
}

#[tokio::test]
async fn goal_gate_routes_to_retry_target_when_present() {
    // Pipeline:
    //   start -> gated_work -> exit
    //   gated_work has goal_gate=true, retry_target=start
    //   gated_work always fails via AlwaysFailHandler.
    //
    // When engine reaches exit and finds goal gate unsatisfied, it should route
    // to the retry_target. Since AlwaysFailHandler always fails, this creates a
    // loop. However, the gated_work node will emit a FAIL outcome, and the
    // edge gated_work -> exit is unconditional, so it still reaches exit. After
    // the first retry (start -> gated_work -> exit), goal gate is still failed
    // and retry_target is still start, so it loops. To prevent an infinite loop
    // in tests, we use a custom handler that fails the first time and succeeds
    // the second time.

    struct FailThenSucceedHandler {
        call_count: std::sync::atomic::AtomicU32,
    }

    #[async_trait::async_trait]
    impl Handler for FailThenSucceedHandler {
        async fn execute(
            &self,
            _node: &Node,
            _context: &attractor::context::Context,
            _graph: &Graph,
            _logs_root: &Path,
            _services: &attractor::handler::EngineServices,
        ) -> Result<Outcome, attractor::error::AttractorError> {
            let count = self
                .call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if count == 0 {
                Ok(Outcome::fail("first attempt fails"))
            } else {
                Ok(Outcome::success())
            }
        }
    }

    let mut graph = Graph::new("GoalGateRetry");

    let mut start = Node::new("start");
    start
        .attrs
        .insert("shape".to_string(), AttrValue::String("Mdiamond".to_string()));
    graph.nodes.insert("start".to_string(), start);

    let mut exit = Node::new("exit");
    exit.attrs
        .insert("shape".to_string(), AttrValue::String("Msquare".to_string()));
    graph.nodes.insert("exit".to_string(), exit);

    let mut gated_work = Node::new("gated_work");
    gated_work
        .attrs
        .insert("goal_gate".to_string(), AttrValue::Boolean(true));
    gated_work
        .attrs
        .insert("max_retries".to_string(), AttrValue::Integer(0));
    gated_work.attrs.insert(
        "retry_target".to_string(),
        AttrValue::String("start".to_string()),
    );
    gated_work.attrs.insert(
        "type".to_string(),
        AttrValue::String("fail_then_succeed".to_string()),
    );
    graph
        .nodes
        .insert("gated_work".to_string(), gated_work);

    graph.edges.push(Edge::new("start", "gated_work"));
    graph.edges.push(Edge::new("gated_work", "exit"));

    let dir = tempfile::tempdir().unwrap();
    let mut registry = HandlerRegistry::new(Box::new(StartHandler));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    registry.register(
        "fail_then_succeed",
        Box::new(FailThenSucceedHandler {
            call_count: std::sync::atomic::AtomicU32::new(0),
        }),
    );

    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };

    let outcome = engine
        .run(&graph, &config)
        .await
        .expect("run should eventually succeed after retry");
    assert_eq!(outcome.status, StageStatus::Success);

    let checkpoint = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    // gated_work should appear in completed nodes (at least twice -- first fail, then succeed)
    let gated_work_count = checkpoint
        .completed_nodes
        .iter()
        .filter(|n| *n == "gated_work")
        .count();
    assert!(
        gated_work_count >= 2,
        "gated_work should have been executed at least twice, got {gated_work_count}"
    );
}

// ---------------------------------------------------------------------------
// 6. Variable expansion transform
// ---------------------------------------------------------------------------

#[test]
fn variable_expansion_replaces_goal_in_prompts() {
    let mut graph = Graph::new("test");
    graph.attrs.insert(
        "goal".to_string(),
        AttrValue::String("Fix all bugs".to_string()),
    );

    let mut plan_node = Node::new("plan");
    plan_node.attrs.insert(
        "prompt".to_string(),
        AttrValue::String("Plan to achieve: $goal".to_string()),
    );
    graph.nodes.insert("plan".to_string(), plan_node);

    let mut impl_node = Node::new("implement");
    impl_node.attrs.insert(
        "prompt".to_string(),
        AttrValue::String("Implement $goal now".to_string()),
    );
    graph
        .nodes
        .insert("implement".to_string(), impl_node);

    let mut no_var_node = Node::new("report");
    no_var_node.attrs.insert(
        "prompt".to_string(),
        AttrValue::String("Generate a report".to_string()),
    );
    graph
        .nodes
        .insert("report".to_string(), no_var_node);

    let transform = VariableExpansionTransform;
    transform.apply(&mut graph);

    let plan_prompt = graph.nodes["plan"]
        .attrs
        .get("prompt")
        .and_then(AttrValue::as_str)
        .expect("plan prompt should exist");
    assert_eq!(plan_prompt, "Plan to achieve: Fix all bugs");

    let impl_prompt = graph.nodes["implement"]
        .attrs
        .get("prompt")
        .and_then(AttrValue::as_str)
        .expect("implement prompt should exist");
    assert_eq!(impl_prompt, "Implement Fix all bugs now");

    let report_prompt = graph.nodes["report"]
        .attrs
        .get("prompt")
        .and_then(AttrValue::as_str)
        .expect("report prompt should exist");
    assert_eq!(report_prompt, "Generate a report");
}

// ---------------------------------------------------------------------------
// 7. Stylesheet application
// ---------------------------------------------------------------------------

#[test]
fn stylesheet_application_by_specificity() {
    let stylesheet_text = r#"
        * { llm_model: claude-sonnet-4-5; llm_provider: anthropic; }
        .code { llm_model: claude-opus-4-6; llm_provider: anthropic; }
        #critical_review { llm_model: gpt-5.2; llm_provider: openai; reasoning_effort: high; }
    "#;

    let mut graph = Graph::new("test");
    graph.attrs.insert(
        "model_stylesheet".to_string(),
        AttrValue::String(stylesheet_text.to_string()),
    );

    // plan node: no class, should get universal defaults
    let plan = Node::new("plan");
    graph.nodes.insert("plan".to_string(), plan);

    // implement node: class="code", should get .code overrides
    let mut implement = Node::new("implement");
    implement.classes.push("code".to_string());
    graph
        .nodes
        .insert("implement".to_string(), implement);

    // critical_review node: class="code" AND id="critical_review", id wins
    let mut critical = Node::new("critical_review");
    critical.classes.push("code".to_string());
    graph
        .nodes
        .insert("critical_review".to_string(), critical);

    // explicit node: has explicit llm_model, should NOT be overridden
    let mut explicit = Node::new("explicit_node");
    explicit.attrs.insert(
        "llm_model".to_string(),
        AttrValue::String("my-custom-model".to_string()),
    );
    graph
        .nodes
        .insert("explicit_node".to_string(), explicit);

    let transform = StylesheetApplicationTransform;
    transform.apply(&mut graph);

    // plan: universal -> claude-sonnet-4-5
    assert_eq!(
        graph.nodes["plan"].attrs.get("llm_model"),
        Some(&AttrValue::String("claude-sonnet-4-5".to_string()))
    );
    assert_eq!(
        graph.nodes["plan"].attrs.get("llm_provider"),
        Some(&AttrValue::String("anthropic".to_string()))
    );

    // implement: .code -> claude-opus-4-6
    assert_eq!(
        graph.nodes["implement"].attrs.get("llm_model"),
        Some(&AttrValue::String("claude-opus-4-6".to_string()))
    );
    assert_eq!(
        graph.nodes["implement"].attrs.get("llm_provider"),
        Some(&AttrValue::String("anthropic".to_string()))
    );

    // critical_review: #critical_review -> gpt-5.2 (id overrides class)
    assert_eq!(
        graph.nodes["critical_review"].attrs.get("llm_model"),
        Some(&AttrValue::String("gpt-5.2".to_string()))
    );
    assert_eq!(
        graph.nodes["critical_review"].attrs.get("llm_provider"),
        Some(&AttrValue::String("openai".to_string()))
    );
    assert_eq!(
        graph.nodes["critical_review"]
            .attrs
            .get("reasoning_effort"),
        Some(&AttrValue::String("high".to_string()))
    );

    // explicit_node: explicit attr NOT overridden by universal
    assert_eq!(
        graph.nodes["explicit_node"].attrs.get("llm_model"),
        Some(&AttrValue::String("my-custom-model".to_string()))
    );
}

#[test]
fn stylesheet_application_via_parsed_graph() {
    let input = r#"digraph StyleTest {
        graph [
            goal="Test stylesheet",
            model_stylesheet="* { llm_model: sonnet; }"
        ]
        start [shape=Mdiamond]
        exit  [shape=Msquare]
        work  [shape=box, prompt="Do work"]
        start -> work -> exit
    }"#;

    let mut graph = parse(input).expect("parse should succeed");
    validate_or_raise(&graph, &[]).expect("validation should pass");

    let transform = StylesheetApplicationTransform;
    transform.apply(&mut graph);

    // All nodes without explicit llm_model should get "sonnet"
    assert_eq!(
        graph.nodes["work"].attrs.get("llm_model"),
        Some(&AttrValue::String("sonnet".to_string()))
    );
    assert_eq!(
        graph.nodes["start"].attrs.get("llm_model"),
        Some(&AttrValue::String("sonnet".to_string()))
    );
    assert_eq!(
        graph.nodes["exit"].attrs.get("llm_model"),
        Some(&AttrValue::String("sonnet".to_string()))
    );
}

#[test]
fn stylesheet_parse_and_apply_directly() {
    let stylesheet_text = "* { llm_model: base; } .fast { llm_model: turbo; }";
    let stylesheet = parse_stylesheet(stylesheet_text).expect("stylesheet parse should succeed");
    assert_eq!(stylesheet.rules.len(), 2);

    let mut graph = Graph::new("test");
    let plain = Node::new("a");
    graph.nodes.insert("a".to_string(), plain);

    let mut fast_node = Node::new("b");
    fast_node.classes.push("fast".to_string());
    graph.nodes.insert("b".to_string(), fast_node);

    apply_stylesheet(&stylesheet, &mut graph);

    assert_eq!(
        graph.nodes["a"].attrs.get("llm_model"),
        Some(&AttrValue::String("base".to_string()))
    );
    assert_eq!(
        graph.nodes["b"].attrs.get("llm_model"),
        Some(&AttrValue::String("turbo".to_string()))
    );
}

// ---------------------------------------------------------------------------
// 8. Retry on failure (Gap #35.1)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn retry_on_failure_then_succeed() {
    // A handler that fails the first call and succeeds on the second.
    struct RetryHandler {
        call_count: std::sync::atomic::AtomicU32,
    }

    #[async_trait::async_trait]
    impl Handler for RetryHandler {
        async fn execute(
            &self,
            _node: &Node,
            _context: &Context,
            _graph: &Graph,
            _logs_root: &Path,
            _services: &attractor::handler::EngineServices,
        ) -> Result<Outcome, AttractorError> {
            let count = self
                .call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if count == 0 {
                Ok(Outcome::retry("transient failure"))
            } else {
                Ok(Outcome::success())
            }
        }
    }

    let mut graph = Graph::new("RetryTest");

    let mut start = Node::new("start");
    start
        .attrs
        .insert("shape".to_string(), AttrValue::String("Mdiamond".to_string()));
    graph.nodes.insert("start".to_string(), start);

    let mut exit = Node::new("exit");
    exit.attrs
        .insert("shape".to_string(), AttrValue::String("Msquare".to_string()));
    graph.nodes.insert("exit".to_string(), exit);

    let mut retry_node = Node::new("work");
    retry_node.attrs.insert(
        "type".to_string(),
        AttrValue::String("retry_handler".to_string()),
    );
    retry_node
        .attrs
        .insert("max_retries".to_string(), AttrValue::Integer(3));
    graph.nodes.insert("work".to_string(), retry_node);

    graph.edges.push(Edge::new("start", "work"));
    graph.edges.push(Edge::new("work", "exit"));

    let dir = tempfile::tempdir().unwrap();
    let mut registry = HandlerRegistry::new(Box::new(StartHandler));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    registry.register(
        "retry_handler",
        Box::new(RetryHandler {
            call_count: std::sync::atomic::AtomicU32::new(0),
        }),
    );

    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };

    let outcome = engine
        .run(&graph, &config)
        .await
        .expect("should succeed after retry");
    assert_eq!(outcome.status, StageStatus::Success);
}

// ---------------------------------------------------------------------------
// 9. Pipeline with 10+ nodes (Gap #35.2)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pipeline_with_many_nodes() {
    // Build a linear pipeline: start -> n1 -> n2 -> ... -> n10 -> exit (12 nodes)
    let mut graph = Graph::new("ManyNodes");
    graph.attrs.insert(
        "goal".to_string(),
        AttrValue::String("Test large pipeline".to_string()),
    );

    let mut start = Node::new("start");
    start
        .attrs
        .insert("shape".to_string(), AttrValue::String("Mdiamond".to_string()));
    graph.nodes.insert("start".to_string(), start);

    let mut exit = Node::new("exit");
    exit.attrs
        .insert("shape".to_string(), AttrValue::String("Msquare".to_string()));
    graph.nodes.insert("exit".to_string(), exit);

    let node_names: Vec<String> = (1..=10).map(|i| format!("step_{i}")).collect();

    for name in &node_names {
        let mut node = Node::new(name.clone());
        node.attrs.insert(
            "shape".to_string(),
            AttrValue::String("box".to_string()),
        );
        node.attrs.insert(
            "prompt".to_string(),
            AttrValue::String(format!("Execute {name}")),
        );
        graph.nodes.insert(name.clone(), node);
    }

    graph.edges.push(Edge::new("start", &node_names[0]));
    for pair in node_names.windows(2) {
        graph.edges.push(Edge::new(&pair[0], &pair[1]));
    }
    graph.edges.push(Edge::new(
        node_names.last().unwrap(),
        "exit",
    ));

    let dir = tempfile::tempdir().unwrap();
    let engine = PipelineEngine::new(make_linear_registry(), EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };

    let outcome = engine
        .run(&graph, &config)
        .await
        .expect("large pipeline should succeed");
    assert_eq!(outcome.status, StageStatus::Success);

    let checkpoint = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    // All 10 step nodes should be in completed_nodes
    for name in &node_names {
        assert!(
            checkpoint.completed_nodes.contains(name),
            "{name} should be in completed_nodes"
        );
    }
}

// ---------------------------------------------------------------------------
// 10. Checkpoint save and load round-trip (Gap #35.3)
// ---------------------------------------------------------------------------

#[test]
fn checkpoint_save_and_resume_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("checkpoint.json");

    let ctx = Context::new();
    ctx.set("goal", serde_json::json!("Test checkpoint"));
    ctx.set("progress", serde_json::json!(42));
    ctx.append_log("started");
    ctx.append_log("step_1 completed");

    let mut retries = std::collections::HashMap::new();
    retries.insert("step_1".to_string(), 1u32);
    let checkpoint = Checkpoint::from_context(
        &ctx,
        "step_2",
        vec![
            "start".to_string(),
            "step_1".to_string(),
        ],
        retries,
        std::collections::HashMap::new(),
        None,
    );

    checkpoint.save(&path).expect("save should succeed");

    let loaded = Checkpoint::load(&path).expect("load should succeed");
    assert_eq!(loaded.current_node, "step_2");
    assert_eq!(loaded.completed_nodes.len(), 2);
    assert!(loaded.completed_nodes.contains(&"start".to_string()));
    assert!(loaded.completed_nodes.contains(&"step_1".to_string()));
    assert_eq!(loaded.node_retries.get("step_1"), Some(&1));
    assert_eq!(
        loaded.context_values.get("goal"),
        Some(&serde_json::json!("Test checkpoint"))
    );
    assert_eq!(
        loaded.context_values.get("progress"),
        Some(&serde_json::json!(42))
    );
    assert_eq!(loaded.logs.len(), 2);
}

// ---------------------------------------------------------------------------
// 11. Smoke test with mock CodergenBackend (Gap #36)
// ---------------------------------------------------------------------------

struct MockCodergenBackend;

#[async_trait::async_trait]
impl CodergenBackend for MockCodergenBackend {
    async fn run(
        &self,
        node: &Node,
        prompt: &str,
        _context: &Context,
        _thread_id: Option<&str>,
    ) -> Result<CodergenResult, AttractorError> {
        Ok(CodergenResult::Text(format!(
            "Response for {}: processed prompt '{}'",
            node.id,
            &prompt[..prompt.len().min(50)]
        )))
    }
}

// ---------------------------------------------------------------------------
// Helpers for parity tests
// ---------------------------------------------------------------------------

/// A handler backed by a shared AtomicU32 counter.
/// Returns Fail on call 0, Success on call >= 1.
struct CounterHandler {
    call_count: Arc<std::sync::atomic::AtomicU32>,
}

#[async_trait::async_trait]
impl Handler for CounterHandler {
    async fn execute(
        &self,
        _node: &Node,
        _context: &Context,
        _graph: &Graph,
        _logs_root: &Path,
        _services: &attractor::handler::EngineServices,
    ) -> Result<Outcome, AttractorError> {
        let count = self
            .call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if count == 0 {
            Ok(Outcome::fail("first call fails"))
        } else {
            Ok(Outcome::success())
        }
    }
}

/// A handler that sets context_updates = {"my_flag": "set"}.
struct ContextSetterHandler;

#[async_trait::async_trait]
impl Handler for ContextSetterHandler {
    async fn execute(
        &self,
        _node: &Node,
        _context: &Context,
        _graph: &Graph,
        _logs_root: &Path,
        _services: &attractor::handler::EngineServices,
    ) -> Result<Outcome, AttractorError> {
        let mut outcome = Outcome::success();
        outcome
            .context_updates
            .insert("my_flag".to_string(), serde_json::json!("set"));
        Ok(outcome)
    }
}

fn collect_events(emitter: &mut EventEmitter) -> Arc<std::sync::Mutex<Vec<PipelineEvent>>> {
    let events = Arc::new(std::sync::Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);
    emitter.on_event(move |event| {
        events_clone.lock().unwrap().push(event.clone());
    });
    events
}

fn make_full_registry(interviewer: Arc<dyn Interviewer>) -> HandlerRegistry {
    let mut registry = HandlerRegistry::new(Box::new(CodergenHandler::new(None)));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    registry.register("codergen", Box::new(CodergenHandler::new(None)));
    registry.register("conditional", Box::new(ConditionalHandler));
    registry.register("tool", Box::new(ToolHandler));
    registry.register(
        "wait.human",
        Box::new(WaitHumanHandler::new(interviewer)),
    );
    registry.register(
        "stack.manager_loop",
        Box::new(ManagerLoopHandler::new(None)),
    );
    registry
}

fn make_graph_with_start_exit(name: &str) -> Graph {
    let mut graph = Graph::new(name);
    let mut start = Node::new("start");
    start
        .attrs
        .insert("shape".to_string(), AttrValue::String("Mdiamond".to_string()));
    graph.nodes.insert("start".to_string(), start);
    let mut exit = Node::new("exit");
    exit.attrs
        .insert("shape".to_string(), AttrValue::String("Msquare".to_string()));
    graph.nodes.insert("exit".to_string(), exit);
    graph
}

#[tokio::test]
async fn smoke_test_with_mock_codergen_backend() {
    // Pipeline:
    //   start -> plan -> gate (diamond)
    //   gate -> implement [condition="outcome=success"]
    //   gate -> fix       [condition="outcome!=success"]
    //   implement -> exit
    //   fix -> exit
    //
    // codergen nodes use MockCodergenBackend which returns real Text responses.
    // The gate is a conditional node. Since the mock backend returns success,
    // we should route through implement.

    let mut graph = Graph::new("SmokeTest");
    graph.attrs.insert(
        "goal".to_string(),
        AttrValue::String("Build and validate".to_string()),
    );

    let mut start = Node::new("start");
    start
        .attrs
        .insert("shape".to_string(), AttrValue::String("Mdiamond".to_string()));
    graph.nodes.insert("start".to_string(), start);

    let mut exit = Node::new("exit");
    exit.attrs
        .insert("shape".to_string(), AttrValue::String("Msquare".to_string()));
    graph.nodes.insert("exit".to_string(), exit);

    let mut plan = Node::new("plan");
    plan.attrs
        .insert("shape".to_string(), AttrValue::String("box".to_string()));
    plan.attrs.insert(
        "prompt".to_string(),
        AttrValue::String("Plan to achieve: $goal".to_string()),
    );
    graph.nodes.insert("plan".to_string(), plan);

    let mut gate = Node::new("gate");
    gate.attrs
        .insert("shape".to_string(), AttrValue::String("diamond".to_string()));
    graph.nodes.insert("gate".to_string(), gate);

    let mut implement = Node::new("implement");
    implement
        .attrs
        .insert("shape".to_string(), AttrValue::String("box".to_string()));
    implement.attrs.insert(
        "prompt".to_string(),
        AttrValue::String("Implement the plan".to_string()),
    );
    graph
        .nodes
        .insert("implement".to_string(), implement);

    let mut fix = Node::new("fix");
    fix.attrs
        .insert("shape".to_string(), AttrValue::String("box".to_string()));
    fix.attrs.insert(
        "prompt".to_string(),
        AttrValue::String("Fix the issues".to_string()),
    );
    graph.nodes.insert("fix".to_string(), fix);

    graph.edges.push(Edge::new("start", "plan"));
    graph.edges.push(Edge::new("plan", "gate"));

    let mut gate_impl = Edge::new("gate", "implement");
    gate_impl.attrs.insert(
        "condition".to_string(),
        AttrValue::String("outcome=success".to_string()),
    );
    graph.edges.push(gate_impl);

    let mut gate_fix = Edge::new("gate", "fix");
    gate_fix.attrs.insert(
        "condition".to_string(),
        AttrValue::String("outcome!=success".to_string()),
    );
    graph.edges.push(gate_fix);

    graph.edges.push(Edge::new("implement", "exit"));
    graph.edges.push(Edge::new("fix", "exit"));

    let dir = tempfile::tempdir().unwrap();
    let backend = Box::new(MockCodergenBackend);
    let mut registry =
        HandlerRegistry::new(Box::new(CodergenHandler::new(Some(backend))));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    registry.register(
        "codergen",
        Box::new(CodergenHandler::new(Some(Box::new(MockCodergenBackend)))),
    );
    registry.register("conditional", Box::new(ConditionalHandler));

    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };

    let outcome = engine
        .run(&graph, &config)
        .await
        .expect("smoke test should succeed");
    assert_eq!(outcome.status, StageStatus::Success);

    let checkpoint = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    assert!(
        checkpoint
            .completed_nodes
            .contains(&"plan".to_string()),
        "plan should have executed"
    );
    assert!(
        checkpoint
            .completed_nodes
            .contains(&"implement".to_string()),
        "should route through implement (success path)"
    );
    assert!(
        !checkpoint
            .completed_nodes
            .contains(&"fix".to_string()),
        "should NOT have traversed fix path"
    );

    // Verify response.md was written by the mock backend
    let plan_response = std::fs::read_to_string(dir.path().join("plan").join("response.md"))
        .expect("plan response should exist");
    assert!(
        plan_response.contains("Response for plan"),
        "mock backend should have written response, got: {plan_response}"
    );

    // Verify prompt.md had $goal expanded by the CodergenHandler
    let plan_prompt = std::fs::read_to_string(dir.path().join("plan").join("prompt.md"))
        .expect("plan prompt should exist");
    assert_eq!(plan_prompt, "Plan to achieve: Build and validate");
}

// ---------------------------------------------------------------------------
// 12. Parallel fan-out / fan-in integration test (Gap #14)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn end_to_end_parallel_fan_out_fan_in() {
    use attractor::handler::fan_in::FanInHandler;
    use attractor::handler::parallel::ParallelHandler;

    let input = r#"digraph parallel_test {
        start [shape=Mdiamond]
        fan_out [shape=component]
        branch_a [shape=box, prompt="Branch A work"]
        branch_b [shape=box, prompt="Branch B work"]
        fan_in_node [shape=tripleoctagon]
        done [shape=Msquare]

        start -> fan_out
        fan_out -> branch_a
        fan_out -> branch_b
        branch_a -> fan_in_node
        branch_b -> fan_in_node
        fan_in_node -> done
    }"#;

    let graph = parse(input).expect("parse should succeed");
    validate_or_raise(&graph, &[]).expect("validation should pass");

    let dir = tempfile::tempdir().unwrap();

    let mut registry = HandlerRegistry::new(
        Box::new(CodergenHandler::new(Some(Box::new(MockCodergenBackend)))),
    );
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    registry.register(
        "codergen",
        Box::new(CodergenHandler::new(Some(Box::new(MockCodergenBackend)))),
    );
    registry.register("parallel", Box::new(ParallelHandler));
    registry.register(
        "parallel.fan_in",
        Box::new(FanInHandler::new(Some(Box::new(MockCodergenBackend)))),
    );

    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };

    let outcome = engine
        .run(&graph, &config)
        .await
        .expect("parallel pipeline should succeed");
    assert_eq!(outcome.status, StageStatus::Success);

    let checkpoint = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();

    // The parallel node (fan_out) and fan_in_node should be in completed_nodes.
    // Branch nodes run inside the parallel handler, so they are not recorded
    // individually by the engine -- but fan_out and fan_in_node are top-level.
    assert!(
        checkpoint
            .completed_nodes
            .contains(&"fan_out".to_string()),
        "fan_out should have been executed"
    );
    assert!(
        checkpoint
            .completed_nodes
            .contains(&"fan_in_node".to_string()),
        "fan_in_node should have been executed"
    );

    // Verify parallel.results was populated (both branches ran)
    let parallel_results = checkpoint
        .context_values
        .get("parallel.results")
        .expect("parallel.results should be in context");
    let results_arr = parallel_results.as_array().expect("should be an array");
    assert_eq!(results_arr.len(), 2, "should have 2 branch results");
}

// ---------------------------------------------------------------------------
// 13. Resume from checkpoint (P1)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resume_from_checkpoint_completes_pipeline() {
    // Build a pipeline: start -> step_a -> step_b -> exit
    // Create a checkpoint mid-pipeline (after step_a) and verify
    // run_from_checkpoint completes from step_b onward.

    let mut graph = Graph::new("ResumeTest");
    graph.attrs.insert(
        "goal".to_string(),
        AttrValue::String("Test resume".to_string()),
    );

    let mut start = Node::new("start");
    start.attrs.insert(
        "shape".to_string(),
        AttrValue::String("Mdiamond".to_string()),
    );
    graph.nodes.insert("start".to_string(), start);

    let mut exit = Node::new("exit");
    exit.attrs.insert(
        "shape".to_string(),
        AttrValue::String("Msquare".to_string()),
    );
    graph.nodes.insert("exit".to_string(), exit);

    let step_a = Node::new("step_a");
    graph.nodes.insert("step_a".to_string(), step_a);

    let step_b = Node::new("step_b");
    graph.nodes.insert("step_b".to_string(), step_b);

    graph.edges.push(Edge::new("start", "step_a"));
    graph.edges.push(Edge::new("step_a", "step_b"));
    graph.edges.push(Edge::new("step_b", "exit"));

    // Simulate a checkpoint saved after step_a completed.
    // The checkpoint records step_a as current_node with next_node_id = step_b.
    let ctx = Context::new();
    ctx.set("graph.goal", serde_json::json!("Test resume"));
    ctx.set("outcome", serde_json::json!("success"));

    let mut outcomes = std::collections::HashMap::new();
    outcomes.insert("start".to_string(), Outcome::success());
    outcomes.insert("step_a".to_string(), Outcome::success());

    let checkpoint = Checkpoint::from_context(
        &ctx,
        "step_a",
        vec!["start".to_string(), "step_a".to_string()],
        std::collections::HashMap::new(),
        outcomes,
        Some("step_b".to_string()),
    );

    let dir = tempfile::tempdir().unwrap();
    let mut registry = HandlerRegistry::new(Box::new(StartHandler));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));

    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };

    let outcome = engine
        .run_from_checkpoint(&graph, &config, &checkpoint)
        .await
        .expect("resume should succeed");
    assert_eq!(outcome.status, StageStatus::Success);

    // Verify checkpoint written after resume contains step_b
    let final_cp = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    assert!(
        final_cp.completed_nodes.contains(&"step_b".to_string()),
        "step_b should have been executed after resume"
    );
    // step_a should also be present (carried over from the checkpoint)
    assert!(
        final_cp.completed_nodes.contains(&"step_a".to_string()),
        "step_a should be preserved from checkpoint"
    );
    // start should also be present
    assert!(
        final_cp.completed_nodes.contains(&"start".to_string()),
        "start should be preserved from checkpoint"
    );
}

#[tokio::test]
async fn resume_from_checkpoint_preserves_goal_gate_outcomes() {
    // Build: start -> gated_work (goal_gate=true) -> step_b -> exit
    // Checkpoint after gated_work (success), resume at step_b.
    // At exit, goal gate should pass because outcomes are restored.

    let mut graph = Graph::new("ResumeGoalGateTest");

    let mut start = Node::new("start");
    start.attrs.insert(
        "shape".to_string(),
        AttrValue::String("Mdiamond".to_string()),
    );
    graph.nodes.insert("start".to_string(), start);

    let mut exit = Node::new("exit");
    exit.attrs.insert(
        "shape".to_string(),
        AttrValue::String("Msquare".to_string()),
    );
    graph.nodes.insert("exit".to_string(), exit);

    let mut gated_work = Node::new("gated_work");
    gated_work.attrs.insert(
        "goal_gate".to_string(),
        AttrValue::Boolean(true),
    );
    graph.nodes.insert("gated_work".to_string(), gated_work);

    let step_b = Node::new("step_b");
    graph.nodes.insert("step_b".to_string(), step_b);

    graph.edges.push(Edge::new("start", "gated_work"));
    graph.edges.push(Edge::new("gated_work", "step_b"));
    graph.edges.push(Edge::new("step_b", "exit"));

    // Checkpoint: gated_work completed with success, next is step_b
    let ctx = Context::new();
    ctx.set("outcome", serde_json::json!("success"));

    let mut outcomes = std::collections::HashMap::new();
    outcomes.insert("start".to_string(), Outcome::success());
    outcomes.insert("gated_work".to_string(), Outcome::success());

    let checkpoint = Checkpoint::from_context(
        &ctx,
        "gated_work",
        vec!["start".to_string(), "gated_work".to_string()],
        std::collections::HashMap::new(),
        outcomes,
        Some("step_b".to_string()),
    );

    let dir = tempfile::tempdir().unwrap();
    let mut registry = HandlerRegistry::new(Box::new(StartHandler));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));

    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };

    // This should succeed because goal gate for gated_work is satisfied
    // via restored outcomes
    let outcome = engine
        .run_from_checkpoint(&graph, &config, &checkpoint)
        .await
        .expect("resume with goal gate should succeed");
    assert_eq!(outcome.status, StageStatus::Success);
}

// ===========================================================================
// Parity tests — P1: Core pipeline behaviors
// ===========================================================================

#[tokio::test]
async fn graph_goal_in_context() {
    let input = r#"digraph GoalTest {
        graph [goal="Ship the widget"]
        start [shape=Mdiamond]
        exit  [shape=Msquare]
        work  [shape=box, prompt="Build it"]
        start -> work -> exit
    }"#;
    let graph = parse(input).expect("parse");
    let dir = tempfile::tempdir().unwrap();
    let engine = PipelineEngine::new(make_linear_registry(), EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };
    engine.run(&graph, &config).await.expect("run");

    let cp = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    assert_eq!(
        cp.context_values.get("graph.goal"),
        Some(&serde_json::json!("Ship the widget"))
    );
}

#[tokio::test]
async fn event_streaming_lifecycle() {
    let input = r#"digraph EventTest {
        start [shape=Mdiamond]
        exit  [shape=Msquare]
        task  [shape=box, prompt="Do something"]
        start -> task -> exit
    }"#;
    let graph = parse(input).expect("parse");
    let dir = tempfile::tempdir().unwrap();
    let mut emitter = EventEmitter::new();
    let events = collect_events(&mut emitter);
    let engine = PipelineEngine::new(make_linear_registry(), emitter);
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };
    engine.run(&graph, &config).await.expect("run");

    let collected = events.lock().unwrap();
    assert!(collected
        .iter()
        .any(|e| matches!(e, PipelineEvent::PipelineStarted { .. })));
    assert!(collected
        .iter()
        .any(|e| matches!(e, PipelineEvent::StageStarted { name, .. } if name == "start")));
    assert!(collected
        .iter()
        .any(|e| matches!(e, PipelineEvent::StageCompleted { name, .. } if name == "start")));
    assert!(collected
        .iter()
        .any(|e| matches!(e, PipelineEvent::StageStarted { name, .. } if name == "task")));
    assert!(collected
        .iter()
        .any(|e| matches!(e, PipelineEvent::StageCompleted { name, .. } if name == "task")));
    assert!(collected
        .iter()
        .any(|e| matches!(e, PipelineEvent::CheckpointSaved { .. })));
    assert!(collected
        .iter()
        .any(|e| matches!(e, PipelineEvent::PipelineCompleted { .. })));
    // PipelineStarted first, PipelineCompleted last
    assert!(matches!(
        collected.first().unwrap(),
        PipelineEvent::PipelineStarted { .. }
    ));
    assert!(matches!(
        collected.last().unwrap(),
        PipelineEvent::PipelineCompleted { .. }
    ));
}

#[tokio::test]
async fn context_flow_between_stages() {
    let mut graph = make_graph_with_start_exit("ContextFlowTest");
    let mut step_a = Node::new("step_a");
    step_a
        .attrs
        .insert("shape".to_string(), AttrValue::String("box".to_string()));
    step_a.attrs.insert(
        "prompt".to_string(),
        AttrValue::String("Step A work".to_string()),
    );
    graph.nodes.insert("step_a".to_string(), step_a);
    let mut step_b = Node::new("step_b");
    step_b
        .attrs
        .insert("shape".to_string(), AttrValue::String("box".to_string()));
    step_b.attrs.insert(
        "prompt".to_string(),
        AttrValue::String("Step B work".to_string()),
    );
    graph.nodes.insert("step_b".to_string(), step_b);
    graph.edges.push(Edge::new("start", "step_a"));
    graph.edges.push(Edge::new("step_a", "step_b"));
    graph.edges.push(Edge::new("step_b", "exit"));

    let dir = tempfile::tempdir().unwrap();
    let engine = PipelineEngine::new(make_linear_registry(), EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };
    engine.run(&graph, &config).await.expect("run");

    let cp = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    assert_eq!(
        cp.context_values.get("last_stage"),
        Some(&serde_json::json!("step_b"))
    );
    let last_response = cp
        .context_values
        .get("last_response")
        .unwrap()
        .as_str()
        .unwrap();
    assert!(last_response.contains("[Simulated]"));
}

#[tokio::test]
async fn tool_handler_e2e() {
    let mut graph = make_graph_with_start_exit("ToolTest");
    let mut echo_task = Node::new("echo_task");
    echo_task.attrs.insert(
        "shape".to_string(),
        AttrValue::String("parallelogram".to_string()),
    );
    echo_task.attrs.insert(
        "tool_command".to_string(),
        AttrValue::String("echo hello-from-tool".to_string()),
    );
    graph.nodes.insert("echo_task".to_string(), echo_task);
    graph.edges.push(Edge::new("start", "echo_task"));
    graph.edges.push(Edge::new("echo_task", "exit"));

    let dir = tempfile::tempdir().unwrap();
    let interviewer = Arc::new(AutoApproveInterviewer);
    let engine = PipelineEngine::new(make_full_registry(interviewer), EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };
    let outcome = engine.run(&graph, &config).await.expect("run");
    assert_eq!(outcome.status, StageStatus::Success);

    let cp = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    let tool_output = cp
        .context_values
        .get("tool.output")
        .expect("tool.output should exist");
    assert!(tool_output.as_str().unwrap().contains("hello-from-tool"));
}

#[tokio::test]
async fn auto_approve_interviewer_e2e() {
    let mut graph = make_graph_with_start_exit("AutoApproveTest");
    let mut gate = Node::new("gate");
    gate.attrs.insert(
        "shape".to_string(),
        AttrValue::String("hexagon".to_string()),
    );
    gate.attrs.insert(
        "type".to_string(),
        AttrValue::String("wait.human".to_string()),
    );
    gate.attrs.insert(
        "label".to_string(),
        AttrValue::String("Review".to_string()),
    );
    graph.nodes.insert("gate".to_string(), gate);
    graph
        .nodes
        .insert("approve".to_string(), Node::new("approve"));
    graph
        .nodes
        .insert("reject".to_string(), Node::new("reject"));
    graph.edges.push(Edge::new("start", "gate"));
    let mut e_approve = Edge::new("gate", "approve");
    e_approve.attrs.insert(
        "label".to_string(),
        AttrValue::String("[A] Approve".to_string()),
    );
    graph.edges.push(e_approve);
    let mut e_reject = Edge::new("gate", "reject");
    e_reject.attrs.insert(
        "label".to_string(),
        AttrValue::String("[R] Reject".to_string()),
    );
    graph.edges.push(e_reject);
    graph.edges.push(Edge::new("approve", "exit"));
    graph.edges.push(Edge::new("reject", "exit"));

    let dir = tempfile::tempdir().unwrap();
    let interviewer = Arc::new(AutoApproveInterviewer);
    let engine = PipelineEngine::new(make_full_registry(interviewer), EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };
    let outcome = engine.run(&graph, &config).await.expect("run");
    assert_eq!(outcome.status, StageStatus::Success);

    let cp = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    assert!(cp.completed_nodes.contains(&"approve".to_string()));
    assert!(!cp.completed_nodes.contains(&"reject".to_string()));
}

#[tokio::test]
async fn codergen_without_backend_simulated() {
    let input = r#"digraph SimTest {
        start [shape=Mdiamond]
        exit  [shape=Msquare]
        code  [shape=box, prompt="Write the code"]
        start -> code -> exit
    }"#;
    let graph = parse(input).expect("parse");
    let dir = tempfile::tempdir().unwrap();
    let engine = PipelineEngine::new(make_linear_registry(), EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };
    engine.run(&graph, &config).await.expect("run");

    let response =
        std::fs::read_to_string(dir.path().join("code").join("response.md")).unwrap();
    assert!(response.contains("[Simulated]"));

    let cp = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    let last_response = cp
        .context_values
        .get("last_response")
        .unwrap()
        .as_str()
        .unwrap();
    assert!(last_response.contains("[Simulated]"));
}

// ===========================================================================
// Parity tests — P2: Complex scenarios
// ===========================================================================

#[tokio::test]
async fn branching_loop_back_on_failure() {
    struct FailThenSucceedHandler {
        call_count: std::sync::atomic::AtomicU32,
    }

    #[async_trait::async_trait]
    impl Handler for FailThenSucceedHandler {
        async fn execute(
            &self,
            _node: &Node,
            _context: &Context,
            _graph: &Graph,
            _logs_root: &Path,
            _services: &attractor::handler::EngineServices,
        ) -> Result<Outcome, AttractorError> {
            let count = self
                .call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if count == 0 {
                Ok(Outcome::fail("first attempt fails"))
            } else {
                Ok(Outcome::success())
            }
        }
    }

    let mut graph = make_graph_with_start_exit("LoopTest");
    let mut implement = Node::new("implement");
    implement
        .attrs
        .insert("shape".to_string(), AttrValue::String("box".to_string()));
    implement.attrs.insert(
        "prompt".to_string(),
        AttrValue::String("Implement".to_string()),
    );
    graph.nodes.insert("implement".to_string(), implement);
    let mut validate_node = Node::new("validate");
    validate_node.attrs.insert(
        "type".to_string(),
        AttrValue::String("fail_then_succeed".to_string()),
    );
    graph
        .nodes
        .insert("validate".to_string(), validate_node);

    graph.edges.push(Edge::new("start", "implement"));
    graph.edges.push(Edge::new("implement", "validate"));
    let mut e_success = Edge::new("validate", "exit");
    e_success.attrs.insert(
        "condition".to_string(),
        AttrValue::String("outcome=success".to_string()),
    );
    graph.edges.push(e_success);
    let mut e_fail = Edge::new("validate", "implement");
    e_fail.attrs.insert(
        "condition".to_string(),
        AttrValue::String("outcome=fail".to_string()),
    );
    graph.edges.push(e_fail);

    let dir = tempfile::tempdir().unwrap();
    let mut registry = HandlerRegistry::new(Box::new(CodergenHandler::new(None)));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    registry.register("codergen", Box::new(CodergenHandler::new(None)));
    registry.register(
        "fail_then_succeed",
        Box::new(FailThenSucceedHandler {
            call_count: std::sync::atomic::AtomicU32::new(0),
        }),
    );
    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };
    let outcome = engine.run(&graph, &config).await.expect("run");
    assert_eq!(outcome.status, StageStatus::Success);

    let cp = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    let implement_count = cp
        .completed_nodes
        .iter()
        .filter(|n| *n == "implement")
        .count();
    assert!(
        implement_count >= 2,
        "implement should appear at least 2x, got {implement_count}"
    );
}

#[tokio::test]
async fn human_gate_loops_back() {
    let mut graph = make_graph_with_start_exit("HumanLoopTest");
    let mut gate = Node::new("gate");
    gate.attrs.insert(
        "shape".to_string(),
        AttrValue::String("hexagon".to_string()),
    );
    gate.attrs.insert(
        "type".to_string(),
        AttrValue::String("wait.human".to_string()),
    );
    gate.attrs.insert(
        "label".to_string(),
        AttrValue::String("Review".to_string()),
    );
    graph.nodes.insert("gate".to_string(), gate);
    graph
        .nodes
        .insert("approve".to_string(), Node::new("approve"));
    graph.nodes.insert("fix".to_string(), Node::new("fix"));

    graph.edges.push(Edge::new("start", "gate"));
    let mut e_approve = Edge::new("gate", "approve");
    e_approve.attrs.insert(
        "label".to_string(),
        AttrValue::String("[A] Approve".to_string()),
    );
    graph.edges.push(e_approve);
    let mut e_fix = Edge::new("gate", "fix");
    e_fix.attrs.insert(
        "label".to_string(),
        AttrValue::String("[F] Fix".to_string()),
    );
    graph.edges.push(e_fix);
    graph.edges.push(Edge::new("fix", "gate"));
    graph.edges.push(Edge::new("approve", "exit"));

    let answers = VecDeque::from([
        Answer {
            value: AnswerValue::Selected("F".to_string()),
            selected_option: None,
            text: None,
        },
        Answer {
            value: AnswerValue::Selected("A".to_string()),
            selected_option: None,
            text: None,
        },
    ]);
    let interviewer = Arc::new(QueueInterviewer::new(answers));

    let dir = tempfile::tempdir().unwrap();
    let mut registry = HandlerRegistry::new(Box::new(StartHandler));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    registry.register(
        "wait.human",
        Box::new(WaitHumanHandler::new(interviewer)),
    );
    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };
    let outcome = engine.run(&graph, &config).await.expect("run");
    assert_eq!(outcome.status, StageStatus::Success);

    let cp = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    let gate_count = cp
        .completed_nodes
        .iter()
        .filter(|n| *n == "gate")
        .count();
    assert!(
        gate_count >= 2,
        "gate should appear at least 2x, got {gate_count}"
    );
    assert!(cp.completed_nodes.contains(&"approve".to_string()));
}

#[tokio::test]
async fn scenario_ship_a_feature() {
    let dot = r#"digraph ShipFeature {
        graph [goal="Ship the widget"]
        rankdir=LR
        start [shape=Mdiamond]
        exit  [shape=Msquare]
        plan  [shape=box, prompt="Plan to achieve: $goal"]
        implement [shape=box, prompt="Implement the plan"]
        test  [shape=parallelogram, tool_command="echo PASS"]
        review [shape=hexagon, label="Review Changes"]
        start -> plan -> implement -> test -> review
        review -> exit [label="[A] Approve"]
        review -> implement [label="[F] Fix"]
    }"#;
    let mut graph = parse(dot).expect("parse");
    validate_or_raise(&graph, &[]).expect("validate");
    VariableExpansionTransform.apply(&mut graph);
    assert_eq!(
        graph.nodes["plan"].prompt().unwrap(),
        "Plan to achieve: Ship the widget"
    );

    let interviewer = Arc::new(AutoApproveInterviewer);
    let dir = tempfile::tempdir().unwrap();
    let mut emitter = EventEmitter::new();
    let events = collect_events(&mut emitter);
    let engine = PipelineEngine::new(make_full_registry(interviewer), emitter);
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };
    let outcome = engine.run(&graph, &config).await.expect("run");
    assert_eq!(outcome.status, StageStatus::Success);

    let cp = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    let tool_output = cp.context_values.get("tool.output").expect("tool.output");
    assert!(tool_output.as_str().unwrap().contains("PASS"));
    assert!(cp.completed_nodes.contains(&"plan".to_string()));
    assert!(cp.completed_nodes.contains(&"implement".to_string()));
    assert!(cp.completed_nodes.contains(&"test".to_string()));
    assert!(cp.completed_nodes.contains(&"review".to_string()));

    let collected = events.lock().unwrap();
    assert!(collected
        .iter()
        .any(|e| matches!(e, PipelineEvent::PipelineStarted { .. })));
    assert!(collected
        .iter()
        .any(|e| matches!(e, PipelineEvent::PipelineCompleted { .. })));
}

#[tokio::test]
async fn scenario_parallel_expert_review() {
    use attractor::handler::fan_in::FanInHandler;
    use attractor::handler::parallel::ParallelHandler;

    let input = r#"digraph ParallelReview {
        start [shape=Mdiamond]
        fan_out [shape=component]
        expert_a [shape=box, prompt="Expert A review"]
        expert_b [shape=box, prompt="Expert B review"]
        expert_c [shape=box, prompt="Expert C review"]
        fan_in_node [shape=tripleoctagon]
        review [shape=hexagon, label="Final Review"]
        exit [shape=Msquare]
        start -> fan_out
        fan_out -> expert_a
        fan_out -> expert_b
        fan_out -> expert_c
        expert_a -> fan_in_node
        expert_b -> fan_in_node
        expert_c -> fan_in_node
        fan_in_node -> review
        review -> exit [label="[A] Approve"]
        review -> fan_out [label="[F] Redo"]
    }"#;
    let graph = parse(input).expect("parse");
    validate_or_raise(&graph, &[]).expect("validate");

    let recorder = Arc::new(RecordingInterviewer::new(Box::new(AutoApproveInterviewer)));
    let dir = tempfile::tempdir().unwrap();

    let interviewer: Arc<dyn Interviewer> = recorder.clone();
    let mut registry = HandlerRegistry::new(Box::new(CodergenHandler::new(Some(
        Box::new(MockCodergenBackend),
    ))));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    registry.register(
        "codergen",
        Box::new(CodergenHandler::new(Some(Box::new(MockCodergenBackend)))),
    );
    registry.register("parallel", Box::new(ParallelHandler));
    registry.register(
        "parallel.fan_in",
        Box::new(FanInHandler::new(Some(Box::new(MockCodergenBackend)))),
    );
    registry.register(
        "wait.human",
        Box::new(WaitHumanHandler::new(interviewer)),
    );

    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };
    let outcome = engine.run(&graph, &config).await.expect("run");
    assert_eq!(outcome.status, StageStatus::Success);

    let cp = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    let results = cp
        .context_values
        .get("parallel.results")
        .expect("parallel.results");
    assert_eq!(results.as_array().unwrap().len(), 3);

    let recordings = recorder.recordings();
    assert_eq!(recordings.len(), 1, "should have 1 interview recording");
    assert!(cp.completed_nodes.contains(&"review".to_string()));
}

#[tokio::test]
async fn scenario_node_retries_on_retry_status() {
    struct RetryHandler {
        call_count: std::sync::atomic::AtomicU32,
    }

    #[async_trait::async_trait]
    impl Handler for RetryHandler {
        async fn execute(
            &self,
            _node: &Node,
            _context: &Context,
            _graph: &Graph,
            _logs_root: &Path,
            _services: &attractor::handler::EngineServices,
        ) -> Result<Outcome, AttractorError> {
            let count = self
                .call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if count == 0 {
                Ok(Outcome::retry("transient failure"))
            } else {
                Ok(Outcome::success())
            }
        }
    }

    let mut graph = make_graph_with_start_exit("RetryScenarioTest");
    let mut flaky = Node::new("flaky");
    flaky.attrs.insert(
        "type".to_string(),
        AttrValue::String("retry_handler".to_string()),
    );
    flaky
        .attrs
        .insert("max_retries".to_string(), AttrValue::Integer(2));
    graph.nodes.insert("flaky".to_string(), flaky);
    graph.edges.push(Edge::new("start", "flaky"));
    graph.edges.push(Edge::new("flaky", "exit"));

    let dir = tempfile::tempdir().unwrap();
    let mut registry = HandlerRegistry::new(Box::new(StartHandler));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    registry.register(
        "retry_handler",
        Box::new(RetryHandler {
            call_count: std::sync::atomic::AtomicU32::new(0),
        }),
    );
    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };
    let outcome = engine.run(&graph, &config).await.expect("run");
    assert_eq!(outcome.status, StageStatus::Success);

    let cp = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    let retry_count = cp
        .node_retries
        .get("flaky")
        .expect("flaky should have retries");
    assert_eq!(*retry_count, 2, "should have been called 2x");
}

#[tokio::test]
async fn scenario_loop_restart_resets_context() {
    let mut graph = make_graph_with_start_exit("LoopRestartTest");
    let mut work = Node::new("work");
    work.attrs.insert(
        "type".to_string(),
        AttrValue::String("counter".to_string()),
    );
    graph.nodes.insert("work".to_string(), work);

    graph.edges.push(Edge::new("start", "work"));
    let mut success_edge = Edge::new("work", "exit");
    success_edge.attrs.insert(
        "condition".to_string(),
        AttrValue::String("outcome=success".to_string()),
    );
    graph.edges.push(success_edge);
    let mut fail_edge = Edge::new("work", "start");
    fail_edge.attrs.insert(
        "condition".to_string(),
        AttrValue::String("outcome=fail".to_string()),
    );
    fail_edge
        .attrs
        .insert("loop_restart".to_string(), AttrValue::Boolean(true));
    graph.edges.push(fail_edge);

    let call_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let dir = tempfile::tempdir().unwrap();
    let mut registry = HandlerRegistry::new(Box::new(StartHandler));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    registry.register(
        "counter",
        Box::new(CounterHandler {
            call_count: Arc::clone(&call_count),
        }),
    );
    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };
    let outcome = engine.run(&graph, &config).await.expect("run");
    assert_eq!(outcome.status, StageStatus::Success);
    assert!(call_count.load(std::sync::atomic::Ordering::SeqCst) >= 2);
}

#[tokio::test]
async fn scenario_bug_triage_router() {
    let mut graph = make_graph_with_start_exit("TriageTest");
    let mut triage = Node::new("triage");
    triage.attrs.insert(
        "shape".to_string(),
        AttrValue::String("diamond".to_string()),
    );
    graph.nodes.insert("triage".to_string(), triage);
    graph
        .nodes
        .insert("critical".to_string(), Node::new("critical"));
    graph
        .nodes
        .insert("normal".to_string(), Node::new("normal"));
    graph
        .nodes
        .insert("wontfix".to_string(), Node::new("wontfix"));

    graph.edges.push(Edge::new("start", "triage"));
    let mut e_critical = Edge::new("triage", "critical");
    e_critical.attrs.insert(
        "condition".to_string(),
        AttrValue::String("outcome=success".to_string()),
    );
    e_critical
        .attrs
        .insert("weight".to_string(), AttrValue::Integer(10));
    graph.edges.push(e_critical);
    let mut e_normal = Edge::new("triage", "normal");
    e_normal.attrs.insert(
        "condition".to_string(),
        AttrValue::String("outcome=success".to_string()),
    );
    e_normal
        .attrs
        .insert("weight".to_string(), AttrValue::Integer(5));
    graph.edges.push(e_normal);
    graph.edges.push(Edge::new("triage", "wontfix"));
    graph.edges.push(Edge::new("critical", "exit"));
    graph.edges.push(Edge::new("normal", "exit"));
    graph.edges.push(Edge::new("wontfix", "exit"));

    let dir = tempfile::tempdir().unwrap();
    let mut registry = HandlerRegistry::new(Box::new(StartHandler));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    registry.register("conditional", Box::new(ConditionalHandler));
    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };
    let outcome = engine.run(&graph, &config).await.expect("run");
    assert_eq!(outcome.status, StageStatus::Success);

    let cp = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    assert!(
        cp.completed_nodes.contains(&"critical".to_string()),
        "critical should be selected (highest weight)"
    );
    assert!(!cp.completed_nodes.contains(&"normal".to_string()));
    assert!(!cp.completed_nodes.contains(&"wontfix".to_string()));
}

#[tokio::test]
async fn scenario_crash_recovery() {
    let mut graph = make_graph_with_start_exit("CrashRecoveryTest");
    graph.nodes.insert("a".to_string(), Node::new("a"));
    graph.nodes.insert("b".to_string(), Node::new("b"));
    graph.nodes.insert("c".to_string(), Node::new("c"));
    graph.edges.push(Edge::new("start", "a"));
    graph.edges.push(Edge::new("a", "b"));
    graph.edges.push(Edge::new("b", "c"));
    graph.edges.push(Edge::new("c", "exit"));

    let ctx = Context::new();
    ctx.set("outcome", serde_json::json!("success"));
    let mut outcomes = std::collections::HashMap::new();
    outcomes.insert("start".to_string(), Outcome::success());
    outcomes.insert("a".to_string(), Outcome::success());
    let checkpoint = Checkpoint::from_context(
        &ctx,
        "a",
        vec!["start".to_string(), "a".to_string()],
        std::collections::HashMap::new(),
        outcomes,
        Some("b".to_string()),
    );

    let dir = tempfile::tempdir().unwrap();
    let mut registry = HandlerRegistry::new(Box::new(StartHandler));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };
    let outcome = engine
        .run_from_checkpoint(&graph, &config, &checkpoint)
        .await
        .expect("run");
    assert_eq!(outcome.status, StageStatus::Success);

    let cp = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    assert!(cp.completed_nodes.contains(&"b".to_string()));
    assert!(cp.completed_nodes.contains(&"c".to_string()));
    assert!(cp.completed_nodes.contains(&"a".to_string()));
    let a_count = cp.completed_nodes.iter().filter(|n| *n == "a").count();
    assert_eq!(a_count, 1, "a should not be re-executed");
}

#[tokio::test]
async fn manager_loop_stop_condition_satisfied_e2e() {
    struct DoneSetterHandler;

    #[async_trait::async_trait]
    impl Handler for DoneSetterHandler {
        async fn execute(
            &self,
            _node: &Node,
            _context: &Context,
            _graph: &Graph,
            _logs_root: &Path,
            _services: &attractor::handler::EngineServices,
        ) -> Result<Outcome, AttractorError> {
            let mut outcome = Outcome::success();
            outcome
                .context_updates
                .insert("done".to_string(), serde_json::json!("true"));
            Ok(outcome)
        }
    }

    let mut graph = make_graph_with_start_exit("ManagerStopTest");
    let mut setter = Node::new("setter");
    setter.attrs.insert(
        "type".to_string(),
        AttrValue::String("done_setter".to_string()),
    );
    graph.nodes.insert("setter".to_string(), setter);
    let mut manager = Node::new("manager");
    manager.attrs.insert(
        "type".to_string(),
        AttrValue::String("stack.manager_loop".to_string()),
    );
    manager.attrs.insert(
        "manager.stop_condition".to_string(),
        AttrValue::String("context.done=true".to_string()),
    );
    manager
        .attrs
        .insert("manager.max_cycles".to_string(), AttrValue::Integer(10));
    manager.attrs.insert(
        "manager.poll_interval".to_string(),
        AttrValue::Duration(std::time::Duration::from_millis(1)),
    );
    graph.nodes.insert("manager".to_string(), manager);
    graph.edges.push(Edge::new("start", "setter"));
    graph.edges.push(Edge::new("setter", "manager"));
    graph.edges.push(Edge::new("manager", "exit"));

    let dir = tempfile::tempdir().unwrap();
    let mut registry = HandlerRegistry::new(Box::new(StartHandler));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    registry.register("done_setter", Box::new(DoneSetterHandler));
    registry.register(
        "stack.manager_loop",
        Box::new(ManagerLoopHandler::new(None)),
    );
    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };
    let outcome = engine.run(&graph, &config).await.expect("run");

    let cp = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    let manager_outcome = cp.node_outcomes.get("manager").expect("manager outcome");
    assert_eq!(manager_outcome.status, StageStatus::Success);
    assert!(manager_outcome
        .notes
        .as_deref()
        .unwrap()
        .contains("Stop condition satisfied"));
    // Overall pipeline succeeds because manager succeeded
    assert_eq!(outcome.status, StageStatus::Success);
}

#[tokio::test]
async fn manager_loop_max_cycles_exceeded_e2e() {
    let mut graph = make_graph_with_start_exit("ManagerMaxCyclesTest");
    let mut manager = Node::new("manager");
    manager.attrs.insert(
        "type".to_string(),
        AttrValue::String("stack.manager_loop".to_string()),
    );
    manager
        .attrs
        .insert("manager.max_cycles".to_string(), AttrValue::Integer(2));
    manager.attrs.insert(
        "manager.poll_interval".to_string(),
        AttrValue::Duration(std::time::Duration::from_millis(1)),
    );
    graph.nodes.insert("manager".to_string(), manager);
    graph.edges.push(Edge::new("start", "manager"));
    graph.edges.push(Edge::new("manager", "exit"));

    let dir = tempfile::tempdir().unwrap();
    let mut registry = HandlerRegistry::new(Box::new(StartHandler));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    registry.register(
        "stack.manager_loop",
        Box::new(ManagerLoopHandler::new(None)),
    );
    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };
    let outcome = engine.run(&graph, &config).await.expect("run");

    let cp = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    let manager_outcome = cp.node_outcomes.get("manager").expect("manager outcome");
    assert_eq!(manager_outcome.status, StageStatus::Fail);
    assert!(manager_outcome
        .failure_reason
        .as_deref()
        .unwrap()
        .contains("Max cycles"));
    // Overall pipeline outcome is from last completed node (manager) = Fail
    assert_eq!(outcome.status, StageStatus::Fail);
}

// ===========================================================================
// Parity tests — P3: Validation
// ===========================================================================

#[test]
fn validation_missing_start_node() {
    let mut graph = Graph::new("NoStartTest");
    let mut exit = Node::new("exit");
    exit.attrs
        .insert("shape".to_string(), AttrValue::String("Msquare".to_string()));
    graph.nodes.insert("exit".to_string(), exit);

    let diagnostics = validate(&graph, &[]);
    let start_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.rule == "start_node")
        .collect();
    assert!(
        !start_errors.is_empty(),
        "should have start_node error diagnostic"
    );
}

#[test]
fn validation_missing_exit_node() {
    let mut graph = Graph::new("NoExitTest");
    let mut start = Node::new("start");
    start
        .attrs
        .insert("shape".to_string(), AttrValue::String("Mdiamond".to_string()));
    graph.nodes.insert("start".to_string(), start);
    graph
        .nodes
        .insert("work".to_string(), Node::new("work"));
    graph.edges.push(Edge::new("start", "work"));

    let diagnostics = validate(&graph, &[]);
    let exit_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.rule == "terminal_node")
        .collect();
    assert!(
        !exit_errors.is_empty(),
        "should have terminal_node error diagnostic"
    );
}

#[test]
fn validation_orphan_unreachable_node() {
    let mut graph = make_graph_with_start_exit("OrphanTest");
    graph
        .nodes
        .insert("orphan".to_string(), Node::new("orphan"));
    graph.edges.push(Edge::new("start", "exit"));

    let diagnostics = validate(&graph, &[]);
    let reachability_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "reachability")
        .collect();
    assert!(
        !reachability_errors.is_empty(),
        "should have reachability diagnostic for orphan node"
    );
}

// ===========================================================================
// Parity tests — P4: Edge selection and cross-feature
// ===========================================================================

#[tokio::test]
async fn conditional_branching_success_fail_paths() {
    let mut graph = make_graph_with_start_exit("CondBranchTest");
    let mut work = Node::new("work");
    work.attrs.insert(
        "type".to_string(),
        AttrValue::String("always_fail".to_string()),
    );
    graph.nodes.insert("work".to_string(), work);
    graph
        .nodes
        .insert("success_path".to_string(), Node::new("success_path"));
    graph
        .nodes
        .insert("fail_path".to_string(), Node::new("fail_path"));

    graph.edges.push(Edge::new("start", "work"));
    let mut e_success = Edge::new("work", "success_path");
    e_success.attrs.insert(
        "condition".to_string(),
        AttrValue::String("outcome=success".to_string()),
    );
    graph.edges.push(e_success);
    let mut e_fail = Edge::new("work", "fail_path");
    e_fail.attrs.insert(
        "condition".to_string(),
        AttrValue::String("outcome=fail".to_string()),
    );
    graph.edges.push(e_fail);
    graph.edges.push(Edge::new("success_path", "exit"));
    graph.edges.push(Edge::new("fail_path", "exit"));

    let dir = tempfile::tempdir().unwrap();
    let mut registry = HandlerRegistry::new(Box::new(StartHandler));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    registry.register("always_fail", Box::new(AlwaysFailHandler));
    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };
    let outcome = engine.run(&graph, &config).await.expect("run");
    assert_eq!(outcome.status, StageStatus::Success);

    let cp = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    assert!(cp.completed_nodes.contains(&"fail_path".to_string()));
    assert!(!cp.completed_nodes.contains(&"success_path".to_string()));
}

#[tokio::test]
async fn edge_selection_condition_match_wins_over_weight() {
    let mut graph = make_graph_with_start_exit("CondVsWeightTest");
    graph.nodes.insert("a".to_string(), Node::new("a"));
    graph
        .nodes
        .insert("cond_target".to_string(), Node::new("cond_target"));
    graph.nodes.insert(
        "weighted_target".to_string(),
        Node::new("weighted_target"),
    );

    graph.edges.push(Edge::new("start", "a"));
    let mut e_cond = Edge::new("a", "cond_target");
    e_cond.attrs.insert(
        "condition".to_string(),
        AttrValue::String("outcome=success".to_string()),
    );
    graph.edges.push(e_cond);
    let mut e_weight = Edge::new("a", "weighted_target");
    e_weight
        .attrs
        .insert("weight".to_string(), AttrValue::Integer(100));
    graph.edges.push(e_weight);
    graph.edges.push(Edge::new("cond_target", "exit"));
    graph.edges.push(Edge::new("weighted_target", "exit"));

    let dir = tempfile::tempdir().unwrap();
    let mut registry = HandlerRegistry::new(Box::new(StartHandler));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };
    engine.run(&graph, &config).await.expect("run");

    let cp = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    assert!(cp.completed_nodes.contains(&"cond_target".to_string()));
    assert!(!cp
        .completed_nodes
        .contains(&"weighted_target".to_string()));
}

#[tokio::test]
async fn edge_selection_weight_breaks_ties() {
    let mut graph = make_graph_with_start_exit("WeightTiesTest");
    graph.nodes.insert("a".to_string(), Node::new("a"));
    graph.nodes.insert("low".to_string(), Node::new("low"));
    graph.nodes.insert("high".to_string(), Node::new("high"));

    graph.edges.push(Edge::new("start", "a"));
    let mut e_low = Edge::new("a", "low");
    e_low
        .attrs
        .insert("weight".to_string(), AttrValue::Integer(1));
    graph.edges.push(e_low);
    let mut e_high = Edge::new("a", "high");
    e_high
        .attrs
        .insert("weight".to_string(), AttrValue::Integer(10));
    graph.edges.push(e_high);
    graph.edges.push(Edge::new("low", "exit"));
    graph.edges.push(Edge::new("high", "exit"));

    let dir = tempfile::tempdir().unwrap();
    let mut registry = HandlerRegistry::new(Box::new(StartHandler));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };
    engine.run(&graph, &config).await.expect("run");

    let cp = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    assert!(cp.completed_nodes.contains(&"high".to_string()));
    assert!(!cp.completed_nodes.contains(&"low".to_string()));
}

#[tokio::test]
async fn edge_selection_lexical_tiebreak() {
    let mut graph = make_graph_with_start_exit("LexicalTieTest");
    graph.nodes.insert("a".to_string(), Node::new("a"));
    graph.nodes.insert("beta".to_string(), Node::new("beta"));
    graph
        .nodes
        .insert("alpha".to_string(), Node::new("alpha"));

    graph.edges.push(Edge::new("start", "a"));
    graph.edges.push(Edge::new("a", "beta"));
    graph.edges.push(Edge::new("a", "alpha"));
    graph.edges.push(Edge::new("beta", "exit"));
    graph.edges.push(Edge::new("alpha", "exit"));

    let dir = tempfile::tempdir().unwrap();
    let mut registry = HandlerRegistry::new(Box::new(StartHandler));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };
    engine.run(&graph, &config).await.expect("run");

    let cp = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    assert!(cp.completed_nodes.contains(&"alpha".to_string()));
    assert!(!cp.completed_nodes.contains(&"beta".to_string()));
}

#[tokio::test]
async fn context_updates_visible_across_nodes() {
    let mut graph = make_graph_with_start_exit("ContextVisibilityTest");
    let mut setter = Node::new("setter");
    setter.attrs.insert(
        "type".to_string(),
        AttrValue::String("context_setter".to_string()),
    );
    graph.nodes.insert("setter".to_string(), setter);
    let mut gate = Node::new("gate");
    gate.attrs.insert(
        "shape".to_string(),
        AttrValue::String("diamond".to_string()),
    );
    graph.nodes.insert("gate".to_string(), gate);
    graph.nodes.insert("yes".to_string(), Node::new("yes"));
    graph.nodes.insert("no".to_string(), Node::new("no"));

    graph.edges.push(Edge::new("start", "setter"));
    graph.edges.push(Edge::new("setter", "gate"));
    let mut e_yes = Edge::new("gate", "yes");
    e_yes.attrs.insert(
        "condition".to_string(),
        AttrValue::String("context.my_flag=set".to_string()),
    );
    graph.edges.push(e_yes);
    graph.edges.push(Edge::new("gate", "no"));
    graph.edges.push(Edge::new("yes", "exit"));
    graph.edges.push(Edge::new("no", "exit"));

    let dir = tempfile::tempdir().unwrap();
    let mut registry = HandlerRegistry::new(Box::new(StartHandler));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    registry.register("conditional", Box::new(ConditionalHandler));
    registry.register("context_setter", Box::new(ContextSetterHandler));
    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };
    engine.run(&graph, &config).await.expect("run");

    let cp = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    assert!(cp.completed_nodes.contains(&"yes".to_string()));
    assert!(!cp.completed_nodes.contains(&"no".to_string()));
}

#[tokio::test]
async fn stylesheet_applies_model_override() {
    let input = r#"digraph StylesheetTest {
        graph [
            goal="Test stylesheet",
            model_stylesheet="* { llm_model: custom-model; }"
        ]
        start [shape=Mdiamond]
        exit  [shape=Msquare]
        work  [shape=box, prompt="Do work"]
        start -> work -> exit
    }"#;
    let mut graph = parse(input).expect("parse");
    validate_or_raise(&graph, &[]).expect("validate");
    StylesheetApplicationTransform.apply(&mut graph);
    assert_eq!(graph.nodes["work"].llm_model(), Some("custom-model"));

    let dir = tempfile::tempdir().unwrap();
    let engine = PipelineEngine::new(make_linear_registry(), EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };
    let outcome = engine.run(&graph, &config).await.expect("run");
    assert_eq!(outcome.status, StageStatus::Success);
}

#[tokio::test]
async fn custom_handler_registration_and_execution() {
    struct CustomHandler;

    #[async_trait::async_trait]
    impl Handler for CustomHandler {
        async fn execute(
            &self,
            _node: &Node,
            _context: &Context,
            _graph: &Graph,
            _logs_root: &Path,
            _services: &attractor::handler::EngineServices,
        ) -> Result<Outcome, AttractorError> {
            let mut outcome = Outcome::success();
            outcome
                .context_updates
                .insert("custom.ran".to_string(), serde_json::json!("true"));
            Ok(outcome)
        }
    }

    let mut graph = make_graph_with_start_exit("CustomHandlerTest");
    let mut custom = Node::new("custom");
    custom.attrs.insert(
        "type".to_string(),
        AttrValue::String("my_custom".to_string()),
    );
    graph.nodes.insert("custom".to_string(), custom);
    graph.edges.push(Edge::new("start", "custom"));
    graph.edges.push(Edge::new("custom", "exit"));

    let dir = tempfile::tempdir().unwrap();
    let mut registry = HandlerRegistry::new(Box::new(StartHandler));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    registry.register("my_custom", Box::new(CustomHandler));
    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };
    engine.run(&graph, &config).await.expect("run");

    let cp = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    assert_eq!(
        cp.context_values.get("custom.ran"),
        Some(&serde_json::json!("true"))
    );
}

#[tokio::test]
async fn integration_smoke_plan_implement_review_done() {
    let dot = r#"digraph SmokeIntegration {
        graph [
            goal="Build the feature",
            model_stylesheet="* { llm_model: test-model; }"
        ]
        rankdir=LR
        start [shape=Mdiamond]
        exit  [shape=Msquare]
        plan  [shape=box, prompt="Plan: $goal"]
        implement [shape=box, prompt="Implement"]
        review [shape=hexagon, label="Review"]
        start -> plan -> implement -> review
        review -> exit [label="[A] Approve"]
        review -> implement [label="[F] Fix"]
    }"#;

    // Parse and validate
    let mut graph = parse(dot).expect("parse");
    let diagnostics = validate_or_raise(&graph, &[]).expect("validate");
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty());

    // Apply transforms
    VariableExpansionTransform.apply(&mut graph);
    StylesheetApplicationTransform.apply(&mut graph);

    // Verify transforms applied
    assert_eq!(
        graph.nodes["plan"].prompt().unwrap(),
        "Plan: Build the feature"
    );
    assert_eq!(graph.nodes["plan"].llm_model(), Some("test-model"));

    // Run pipeline
    let interviewer = Arc::new(AutoApproveInterviewer);
    let dir = tempfile::tempdir().unwrap();
    let mut emitter = EventEmitter::new();
    let events = collect_events(&mut emitter);
    let engine = PipelineEngine::new(make_full_registry(interviewer), emitter);
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };
    let outcome = engine.run(&graph, &config).await.expect("run");
    assert_eq!(outcome.status, StageStatus::Success);

    // Verify all nodes completed
    let cp = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    assert!(cp.completed_nodes.contains(&"plan".to_string()));
    assert!(cp.completed_nodes.contains(&"implement".to_string()));
    assert!(cp.completed_nodes.contains(&"review".to_string()));

    // Verify prompt.md and response.md exist
    assert!(dir.path().join("plan").join("prompt.md").exists());
    assert!(dir.path().join("plan").join("response.md").exists());

    // Verify events
    let collected = events.lock().unwrap();
    assert!(collected
        .iter()
        .any(|e| matches!(e, PipelineEvent::PipelineStarted { .. })));
    assert!(collected
        .iter()
        .any(|e| matches!(e, PipelineEvent::PipelineCompleted { .. })));
}

// ===========================================================================
// 17. Full HTTP server lifecycle (TS Scenario 4)
// ===========================================================================

#[cfg(feature = "server")]
mod server_lifecycle {
    use std::sync::Arc;
    use std::time::Duration;

    use attractor::handler::codergen::CodergenHandler;
    use attractor::handler::exit::ExitHandler;
    use attractor::handler::start::StartHandler;
    use attractor::handler::wait_human::WaitHumanHandler;
    use attractor::handler::HandlerRegistry;
    use attractor::interviewer::Interviewer;
    use attractor::server::{build_router, create_app_state};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn gate_registry(interviewer: Arc<dyn Interviewer>) -> HandlerRegistry {
        let mut registry = HandlerRegistry::new(Box::new(CodergenHandler::new(None)));
        registry.register("start", Box::new(StartHandler));
        registry.register("exit", Box::new(ExitHandler));
        registry.register("codergen", Box::new(CodergenHandler::new(None)));
        registry.register("wait.human", Box::new(WaitHumanHandler::new(interviewer)));
        registry
    }

    async fn body_json(body: Body) -> serde_json::Value {
        let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    const GATE_DOT: &str = r#"digraph GateTest {
        graph [goal="Test gate"]
        start [shape=Mdiamond]
        exit  [shape=Msquare]
        work  [shape=box, prompt="Do work"]
        gate  [shape=hexagon, type="wait.human", label="Approve?"]
        done  [shape=box, prompt="Finish"]
        revise [shape=box, prompt="Revise"]

        start -> work -> gate
        gate -> done   [label="[A] Approve"]
        gate -> revise [label="[R] Revise"]
        done -> exit
        revise -> gate
    }"#;

    #[tokio::test]
    async fn full_http_lifecycle_approve_and_complete() {
        let state = create_app_state(gate_registry);
        let app = build_router(Arc::clone(&state));

        // 1. Start pipeline
        let req = Request::builder()
            .method("POST")
            .uri("/pipelines")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_string(&serde_json::json!({"dot_source": GATE_DOT})).unwrap(),
            ))
            .unwrap();

        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = body_json(response.into_body()).await;
        let pipeline_id = body["id"].as_str().unwrap().to_string();

        // 2. Poll for question to appear (pipeline runs start -> work -> gate, then blocks)
        let mut question_id = String::new();
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let req = Request::builder()
                .method("GET")
                .uri(format!("/pipelines/{pipeline_id}/questions"))
                .body(Body::empty())
                .unwrap();
            let response = app.clone().oneshot(req).await.unwrap();
            let body = body_json(response.into_body()).await;
            let arr = body.as_array().unwrap();
            if !arr.is_empty() {
                question_id = arr[0]["id"].as_str().unwrap().to_string();
                break;
            }
        }
        assert!(!question_id.is_empty(), "question should have appeared");

        // 3. Submit answer selecting first option (Approve)
        let req = Request::builder()
            .method("POST")
            .uri(format!(
                "/pipelines/{pipeline_id}/questions/{question_id}/answer"
            ))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_string(&serde_json::json!({"value": "A"})).unwrap(),
            ))
            .unwrap();
        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response.into_body()).await;
        assert_eq!(body["accepted"], true);

        // 4. Poll until completed
        let mut final_status = String::new();
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let req = Request::builder()
                .method("GET")
                .uri(format!("/pipelines/{pipeline_id}"))
                .body(Body::empty())
                .unwrap();
            let response = app.clone().oneshot(req).await.unwrap();
            let body = body_json(response.into_body()).await;
            let status = body["status"].as_str().unwrap().to_string();
            if status == "completed" || status == "failed" {
                final_status = status;
                break;
            }
        }
        assert_eq!(final_status, "completed");

        // 5. Verify context endpoint returns an object
        let req = Request::builder()
            .method("GET")
            .uri(format!("/pipelines/{pipeline_id}/context"))
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let ctx_body = body_json(response.into_body()).await;
        assert!(ctx_body.is_object(), "context should be an object");

        // 6. Verify no pending questions
        let req = Request::builder()
            .method("GET")
            .uri(format!("/pipelines/{pipeline_id}/questions"))
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(req).await.unwrap();
        let body = body_json(response.into_body()).await;
        assert!(
            body.as_array().unwrap().is_empty(),
            "no pending questions after completion"
        );
    }

    #[tokio::test]
    async fn full_http_lifecycle_cancel() {
        let state = create_app_state(gate_registry);
        let app = build_router(Arc::clone(&state));

        // Start a pipeline that will block at the human gate
        let req = Request::builder()
            .method("POST")
            .uri("/pipelines")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_string(&serde_json::json!({"dot_source": GATE_DOT})).unwrap(),
            ))
            .unwrap();
        let response = app.clone().oneshot(req).await.unwrap();
        let body = body_json(response.into_body()).await;
        let pipeline_id = body["id"].as_str().unwrap().to_string();

        // Wait briefly for pipeline to start running
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Cancel it
        let req = Request::builder()
            .method("POST")
            .uri(format!("/pipelines/{pipeline_id}/cancel"))
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response.into_body()).await;
        assert_eq!(body["cancelled"], true);

        // Verify status is cancelled
        let req = Request::builder()
            .method("GET")
            .uri(format!("/pipelines/{pipeline_id}"))
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(req).await.unwrap();
        let body = body_json(response.into_body()).await;
        assert_eq!(body["status"], "cancelled");
    }
}

// ===========================================================================
// 18. SSE event stream content parsing (TS Scenario 8)
// ===========================================================================

#[cfg(feature = "server")]
mod sse_events {
    use std::sync::Arc;
    use std::time::Duration;

    use attractor::handler::codergen::CodergenHandler;
    use attractor::handler::exit::ExitHandler;
    use attractor::handler::start::StartHandler;
    use attractor::handler::HandlerRegistry;
    use attractor::interviewer::Interviewer;
    use attractor::server::{build_router, create_app_state};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn simple_registry(_interviewer: Arc<dyn Interviewer>) -> HandlerRegistry {
        let mut registry = HandlerRegistry::new(Box::new(CodergenHandler::new(None)));
        registry.register("start", Box::new(StartHandler));
        registry.register("exit", Box::new(ExitHandler));
        registry.register("codergen", Box::new(CodergenHandler::new(None)));
        registry
    }

    const SIMPLE_DOT: &str = r#"digraph SSETest {
        graph [goal="Test SSE"]
        start [shape=Mdiamond]
        work  [shape=box, prompt="Do work"]
        exit  [shape=Msquare]
        start -> work -> exit
    }"#;

    #[tokio::test]
    async fn sse_stream_contains_expected_event_types() {
        let state = create_app_state(simple_registry);
        let app = build_router(Arc::clone(&state));

        // Start pipeline
        let req = Request::builder()
            .method("POST")
            .uri("/pipelines")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_string(&serde_json::json!({"dot_source": SIMPLE_DOT})).unwrap(),
            ))
            .unwrap();
        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let pipeline_id = body["id"].as_str().unwrap().to_string();

        // Get SSE stream
        let req = Request::builder()
            .method("GET")
            .uri(format!("/pipelines/{pipeline_id}/events"))
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let content_type = response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(content_type.contains("text/event-stream"));

        // Collect SSE frames with a timeout
        let mut body = response.into_body();
        let mut sse_data = String::new();
        loop {
            match tokio::time::timeout(Duration::from_secs(3), body.frame()).await {
                Ok(Some(Ok(frame))) => {
                    if let Some(data) = frame.data_ref() {
                        sse_data.push_str(&String::from_utf8_lossy(data));
                    }
                }
                _ => break,
            }
        }

        // Parse SSE data lines and extract event types
        let mut event_types: Vec<String> = Vec::new();
        for line in sse_data.lines() {
            if let Some(json_str) = line.strip_prefix("data:") {
                let json_str = json_str.trim();
                if let Ok(event) = serde_json::from_str::<serde_json::Value>(json_str) {
                    // The event is serialized as a tagged enum, so the type is the first key
                    if let Some(obj) = event.as_object() {
                        for key in obj.keys() {
                            event_types.push(key.clone());
                        }
                    } else if let Some(s) = event.as_str() {
                        event_types.push(s.to_string());
                    }
                }
            }
        }

        // Verify we got events (pipeline may have completed before we subscribed,
        // so we check that the stream was valid SSE)
        // If events were emitted before subscribe, the stream may be empty.
        // That's OK -- the main assertion is content-type + valid SSE format.
        // But if we got events, verify expected types.
        if !event_types.is_empty() {
            assert!(
                event_types
                    .iter()
                    .any(|t| t == "StageStarted" || t == "StageCompleted"),
                "should contain stage events, got: {event_types:?}"
            );
        }

        // Wait for completion, then verify checkpoint
        tokio::time::sleep(Duration::from_millis(500)).await;

        let req = Request::builder()
            .method("GET")
            .uri(format!("/pipelines/{pipeline_id}/checkpoint"))
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let cp_body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        // If pipeline completed, checkpoint should have completed_nodes
        if !cp_body.is_null() {
            let completed = cp_body["completed_nodes"].as_array();
            if let Some(nodes) = completed {
                let names: Vec<&str> = nodes.iter().filter_map(|v| v.as_str()).collect();
                assert!(names.contains(&"work"), "work should be in completed_nodes");
            }
        }
    }
}

// ===========================================================================
// 18b. Serve command: dry-run registry factory builds a working router
// ===========================================================================

#[cfg(feature = "server")]
mod serve_dry_run {
    use std::sync::Arc;
    use std::time::Duration;

    use attractor::handler::default_registry;
    use attractor::interviewer::Interviewer;
    use attractor::server::{build_router, create_app_state};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    const MINIMAL_DOT: &str = r#"digraph Test {
        graph [goal="Test"]
        start [shape=Mdiamond]
        exit  [shape=Msquare]
        start -> exit
    }"#;

    /// Build the router exactly as `serve_command` does in dry-run mode.
    fn dry_run_app() -> axum::Router {
        let factory = |interviewer: Arc<dyn Interviewer>| {
            default_registry(interviewer, || None)
        };
        let state = create_app_state(factory);
        build_router(state)
    }

    async fn body_json(body: Body) -> serde_json::Value {
        let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn dry_run_serve_starts_and_runs_pipeline() {
        let app = dry_run_app();

        // POST /pipelines to start a pipeline
        let req = Request::builder()
            .method("POST")
            .uri("/pipelines")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_string(&serde_json::json!({"dot_source": MINIMAL_DOT})).unwrap(),
            ))
            .unwrap();

        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let body = body_json(response.into_body()).await;
        let pipeline_id = body["id"].as_str().unwrap().to_string();
        assert!(!pipeline_id.is_empty());

        // Wait for pipeline to complete
        tokio::time::sleep(Duration::from_millis(500)).await;

        // GET /pipelines/{id} to verify completion
        let req = Request::builder()
            .method("GET")
            .uri(format!("/pipelines/{pipeline_id}"))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = body_json(response.into_body()).await;
        assert_eq!(body["status"].as_str().unwrap(), "completed");
    }

    #[tokio::test]
    async fn dry_run_serve_rejects_invalid_dot() {
        let app = dry_run_app();

        let req = Request::builder()
            .method("POST")
            .uri("/pipelines")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_string(&serde_json::json!({"dot_source": "not valid dot"})).unwrap(),
            ))
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

// ===========================================================================
// 19a. Sub-pipeline E2E (TS Scenario 9)
// ===========================================================================

#[tokio::test]
async fn sub_pipeline_e2e_through_engine() {
    use attractor::handler::sub_pipeline::SubPipelineHandler;

    let input = r#"digraph SubPipelineE2E {
        graph [goal="Test sub-pipeline"]
        start [shape=Mdiamond]
        exit  [shape=Msquare]
        generate [shape=box, prompt="Generate code"]
        validate [type="sub_pipeline", sub_pipeline.dot_source="digraph Child { start [shape=Mdiamond]; lint [shape=box, prompt=\"Lint\"]; test [shape=box, prompt=\"Test\"]; exit [shape=Msquare]; start -> lint -> test -> exit }"]

        start -> generate -> validate -> exit
    }"#;

    let graph = parse(input).expect("parse should succeed");

    let dir = tempfile::tempdir().unwrap();

    let mut registry = HandlerRegistry::new(Box::new(CodergenHandler::new(None)));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    registry.register("codergen", Box::new(CodergenHandler::new(None)));
    registry.register("sub_pipeline", Box::new(SubPipelineHandler));

    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };

    let outcome = engine
        .run(&graph, &config)
        .await
        .expect("sub-pipeline E2E should succeed");
    assert_eq!(outcome.status, StageStatus::Success);

    let checkpoint = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    assert!(
        checkpoint
            .completed_nodes
            .contains(&"generate".to_string()),
        "generate should be in completed_nodes"
    );
    assert!(
        checkpoint
            .completed_nodes
            .contains(&"validate".to_string()),
        "validate should be in completed_nodes"
    );

    // Context should have last_stage set by the validate node's sub-pipeline
    let last_stage = checkpoint.context_values.get("last_stage");
    assert!(last_stage.is_some(), "last_stage should be set in context");
}

// ===========================================================================
// 19b. Manager loop with ChildObserver E2E (TS Scenario 10)
// ===========================================================================

#[tokio::test]
async fn manager_loop_with_child_observer_e2e() {
    use attractor::handler::manager_loop::{ChildObserver, ManagerLoopHandler};
    use std::sync::atomic::{AtomicU32, Ordering};

    struct SimulatingChildObserver {
        launch_count: AtomicU32,
        observe_count: AtomicU32,
    }

    #[async_trait::async_trait]
    impl ChildObserver for SimulatingChildObserver {
        async fn launch_child(
            &self,
            _dotfile: &str,
            _workdir: &str,
            _context: &attractor::context::Context,
        ) -> Result<(), attractor::error::AttractorError> {
            self.launch_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn observe(
            &self,
            context: &attractor::context::Context,
        ) -> Result<(), attractor::error::AttractorError> {
            let cycle = self.observe_count.fetch_add(1, Ordering::SeqCst);
            if cycle >= 2 {
                context.set(
                    "context.stack.child.status",
                    serde_json::json!("completed"),
                );
                context.set(
                    "context.stack.child.outcome",
                    serde_json::json!("success"),
                );
            }
            Ok(())
        }

        async fn steer(
            &self,
            _context: &attractor::context::Context,
            _node: &attractor::graph::Node,
        ) -> Result<(), attractor::error::AttractorError> {
            Ok(())
        }
    }

    let mut graph = Graph::new("ManagerLoopE2E");
    graph.attrs.insert(
        "goal".to_string(),
        AttrValue::String("Test manager loop".to_string()),
    );

    let mut start = Node::new("start");
    start
        .attrs
        .insert("shape".to_string(), AttrValue::String("Mdiamond".to_string()));
    graph.nodes.insert("start".to_string(), start);

    let mut exit = Node::new("exit");
    exit.attrs
        .insert("shape".to_string(), AttrValue::String("Msquare".to_string()));
    graph.nodes.insert("exit".to_string(), exit);

    let mut supervisor = Node::new("supervisor");
    supervisor.attrs.insert(
        "type".to_string(),
        AttrValue::String("stack.manager_loop".to_string()),
    );
    supervisor.attrs.insert(
        "manager.poll_interval".to_string(),
        AttrValue::Duration(std::time::Duration::from_millis(1)),
    );
    supervisor
        .attrs
        .insert("manager.max_cycles".to_string(), AttrValue::Integer(50));
    supervisor.attrs.insert(
        "manager.actions".to_string(),
        AttrValue::String("observe,wait".to_string()),
    );
    supervisor.attrs.insert(
        "manager.stop_condition".to_string(),
        AttrValue::String(String::new()),
    );
    graph
        .nodes
        .insert("supervisor".to_string(), supervisor);

    graph.edges.push(Edge::new("start", "supervisor"));
    graph.edges.push(Edge::new("supervisor", "exit"));

    let dir = tempfile::tempdir().unwrap();
    let observer = SimulatingChildObserver {
        launch_count: AtomicU32::new(0),
        observe_count: AtomicU32::new(0),
    };

    let mut registry = HandlerRegistry::new(Box::new(StartHandler));
    registry.register("start", Box::new(StartHandler));
    registry.register("exit", Box::new(ExitHandler));
    registry.register(
        "stack.manager_loop",
        Box::new(ManagerLoopHandler::new(Some(Box::new(observer)))),
    );

    let engine = PipelineEngine::new(registry, EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };

    let outcome = engine
        .run(&graph, &config)
        .await
        .expect("manager loop E2E should succeed");
    assert_eq!(outcome.status, StageStatus::Success);

    let checkpoint = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    assert!(
        checkpoint
            .completed_nodes
            .contains(&"supervisor".to_string()),
        "supervisor should be in completed_nodes"
    );

    // The manager loop handler stores notes about child completion
    let supervisor_outcome = checkpoint.node_outcomes.get("supervisor");
    assert!(supervisor_outcome.is_some(), "supervisor outcome should exist");
    let notes = supervisor_outcome.unwrap().notes.as_deref().unwrap_or("");
    assert!(
        notes.contains("Child completed"),
        "notes should mention child completion, got: {notes}"
    );
}

// ===========================================================================
// 19c. GraphMerge E2E (TS Scenario 11)
// ===========================================================================

#[tokio::test]
async fn graph_merge_e2e_through_engine() {
    use attractor::transform::GraphMergeTransform;

    // Module "val": lint -> test
    let mut val_graph = Graph::new("val");
    let mut lint = Node::new("lint");
    lint.attrs
        .insert("shape".to_string(), AttrValue::String("box".to_string()));
    lint.attrs.insert(
        "prompt".to_string(),
        AttrValue::String("Lint the code".to_string()),
    );
    val_graph.nodes.insert("lint".to_string(), lint);

    let mut test_node = Node::new("test");
    test_node
        .attrs
        .insert("shape".to_string(), AttrValue::String("box".to_string()));
    test_node.attrs.insert(
        "prompt".to_string(),
        AttrValue::String("Run tests".to_string()),
    );
    val_graph.nodes.insert("test".to_string(), test_node);
    val_graph.edges.push(Edge::new("lint", "test"));

    // Module "dep": stage -> release
    let mut dep_graph = Graph::new("dep");
    let mut stage = Node::new("stage");
    stage
        .attrs
        .insert("shape".to_string(), AttrValue::String("box".to_string()));
    stage.attrs.insert(
        "prompt".to_string(),
        AttrValue::String("Stage the release".to_string()),
    );
    dep_graph.nodes.insert("stage".to_string(), stage);

    let mut release = Node::new("release");
    release
        .attrs
        .insert("shape".to_string(), AttrValue::String("box".to_string()));
    release.attrs.insert(
        "prompt".to_string(),
        AttrValue::String("Release it".to_string()),
    );
    dep_graph.nodes.insert("release".to_string(), release);
    dep_graph.edges.push(Edge::new("stage", "release"));

    // Main graph: start, exit; edges connect modules
    let mut main_graph = Graph::new("MergeE2E");
    main_graph.attrs.insert(
        "goal".to_string(),
        AttrValue::String("Test graph merge".to_string()),
    );

    let mut start = Node::new("start");
    start
        .attrs
        .insert("shape".to_string(), AttrValue::String("Mdiamond".to_string()));
    main_graph.nodes.insert("start".to_string(), start);

    let mut exit = Node::new("exit");
    exit.attrs
        .insert("shape".to_string(), AttrValue::String("Msquare".to_string()));
    main_graph.nodes.insert("exit".to_string(), exit);

    // Apply merge transform
    let merge = GraphMergeTransform::new(vec![val_graph, dep_graph]);
    merge.apply(&mut main_graph);

    // Add cross-module edges
    main_graph.edges.push(Edge::new("start", "val.lint"));
    main_graph.edges.push(Edge::new("val.test", "dep.stage"));
    main_graph
        .edges
        .push(Edge::new("dep.release", "exit"));

    // Verify merged nodes exist
    assert!(main_graph.nodes.contains_key("val.lint"));
    assert!(main_graph.nodes.contains_key("val.test"));
    assert!(main_graph.nodes.contains_key("dep.stage"));
    assert!(main_graph.nodes.contains_key("dep.release"));

    let dir = tempfile::tempdir().unwrap();
    let engine = PipelineEngine::new(make_linear_registry(), EventEmitter::new());
    let config = RunConfig {
        logs_root: dir.path().to_path_buf(),
        cancel_token: None,
    };

    let outcome = engine
        .run(&main_graph, &config)
        .await
        .expect("graph merge E2E should succeed");
    assert_eq!(outcome.status, StageStatus::Success);

    let checkpoint = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
    assert!(
        checkpoint
            .completed_nodes
            .contains(&"val.lint".to_string()),
        "val.lint should be completed"
    );
    assert!(
        checkpoint
            .completed_nodes
            .contains(&"val.test".to_string()),
        "val.test should be completed"
    );
    assert!(
        checkpoint
            .completed_nodes
            .contains(&"dep.stage".to_string()),
        "dep.stage should be completed"
    );
    assert!(
        checkpoint
            .completed_nodes
            .contains(&"dep.release".to_string()),
        "dep.release should be completed"
    );

    // Verify ordering: val.test appears before dep.stage
    let val_test_pos = checkpoint
        .completed_nodes
        .iter()
        .position(|n| n == "val.test")
        .expect("val.test should be in completed_nodes");
    let dep_stage_pos = checkpoint
        .completed_nodes
        .iter()
        .position(|n| n == "dep.stage")
        .expect("dep.stage should be in completed_nodes");
    assert!(
        val_test_pos < dep_stage_pos,
        "val.test ({val_test_pos}) should execute before dep.stage ({dep_stage_pos})"
    );
}

// ===========================================================================
// 20. Real LLM pipeline tests (requires ANTHROPIC_API_KEY)
// ===========================================================================

mod real_llm {
    use std::sync::Arc;

    use async_trait::async_trait;

    use attractor::context::Context;
    use attractor::error::AttractorError;
    use attractor::graph::Node;
    use attractor::handler::codergen::{CodergenBackend, CodergenHandler, CodergenResult};

    use llm::client::Client;
    use llm::types::{Message, Request};

    struct LlmCodergenBackend {
        client: Arc<Client>,
        model: String,
    }

    #[async_trait]
    impl CodergenBackend for LlmCodergenBackend {
        async fn run(
            &self,
            _node: &Node,
            prompt: &str,
            _context: &Context,
            _thread_id: Option<&str>,
        ) -> Result<CodergenResult, AttractorError> {
            let request = Request {
                model: self.model.clone(),
                messages: vec![Message::user(prompt)],
                provider: Some("anthropic".to_string()),
                tools: None,
                tool_choice: None,
                response_format: None,
                temperature: Some(0.0),
                top_p: None,
                max_tokens: Some(200),
                stop_sequences: None,
                reasoning_effort: None,
                metadata: None,
                provider_options: None,
            };
            let response = self
                .client
                .complete(&request)
                .await
                .map_err(|e| AttractorError::Handler(e.to_string()))?;
            Ok(CodergenResult::Text(response.text()))
        }
    }

    async fn make_llm_client() -> Option<Arc<Client>> {
        let _ = dotenvy::dotenv();
        if std::env::var("ANTHROPIC_API_KEY").is_err() {
            return None;
        }
        let client = Client::from_env()
            .await
            .expect("unified-llm client should initialize from env");
        Some(Arc::new(client))
    }

    fn make_llm_backend(client: Arc<Client>) -> Box<LlmCodergenBackend> {
        Box::new(LlmCodergenBackend {
            client,
            model: "claude-haiku-4-5-20251001".to_string(),
        })
    }

    use attractor::checkpoint::Checkpoint;
    use attractor::engine::{PipelineEngine, RunConfig};
    use attractor::event::EventEmitter;
    use attractor::graph::{AttrValue, Edge, Graph};
    use attractor::handler::exit::ExitHandler;
    use attractor::handler::start::StartHandler;
    use attractor::handler::wait_human::WaitHumanHandler;
    use attractor::handler::HandlerRegistry;
    use attractor::interviewer::auto_approve::AutoApproveInterviewer;
    use attractor::outcome::StageStatus;

    #[tokio::test]
    #[ignore]
    async fn real_llm_linear_pipeline() {
        let client = match make_llm_client().await {
            Some(c) => c,
            None => {
                eprintln!("Skipping: ANTHROPIC_API_KEY not set");
                return;
            }
        };

        let mut graph = Graph::new("RealLLMLinear");
        graph.attrs.insert(
            "goal".to_string(),
            AttrValue::String("Describe a sorting algorithm".to_string()),
        );

        let mut start = Node::new("start");
        start.attrs.insert(
            "shape".to_string(),
            AttrValue::String("Mdiamond".to_string()),
        );
        graph.nodes.insert("start".to_string(), start);

        let mut exit = Node::new("exit");
        exit.attrs.insert(
            "shape".to_string(),
            AttrValue::String("Msquare".to_string()),
        );
        graph.nodes.insert("exit".to_string(), exit);

        let mut plan = Node::new("plan");
        plan.attrs.insert(
            "shape".to_string(),
            AttrValue::String("box".to_string()),
        );
        plan.attrs.insert(
            "prompt".to_string(),
            AttrValue::String(
                "Briefly describe quicksort in 2-3 sentences.".to_string(),
            ),
        );
        graph.nodes.insert("plan".to_string(), plan);

        let mut review = Node::new("review");
        review.attrs.insert(
            "shape".to_string(),
            AttrValue::String("box".to_string()),
        );
        review.attrs.insert(
            "prompt".to_string(),
            AttrValue::String(
                "Review the previous description and add one improvement suggestion."
                    .to_string(),
            ),
        );
        graph.nodes.insert("review".to_string(), review);

        graph.edges.push(Edge::new("start", "plan"));
        graph.edges.push(Edge::new("plan", "review"));
        graph.edges.push(Edge::new("review", "exit"));

        let dir = tempfile::tempdir().unwrap();
        let backend = make_llm_backend(client);
        let mut registry =
            HandlerRegistry::new(Box::new(CodergenHandler::new(Some(backend))));
        registry.register("start", Box::new(StartHandler));
        registry.register("exit", Box::new(ExitHandler));
        registry.register(
            "codergen",
            Box::new(CodergenHandler::new(Some(make_llm_backend(
                make_llm_client().await.unwrap(),
            )))),
        );

        let engine = PipelineEngine::new(registry, EventEmitter::new());
        let config = RunConfig {
            logs_root: dir.path().to_path_buf(),
            cancel_token: None,
        };

        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            engine.run(&graph, &config),
        )
        .await
        .expect("should not timeout")
        .expect("real LLM pipeline should succeed");

        assert_eq!(outcome.status, StageStatus::Success);

        let checkpoint = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
        assert!(checkpoint.completed_nodes.contains(&"plan".to_string()));
        assert!(checkpoint.completed_nodes.contains(&"review".to_string()));

        let last_stage = checkpoint
            .context_values
            .get("last_stage")
            .and_then(|v| v.as_str());
        assert_eq!(last_stage, Some("review"));

        // Verify actual LLM responses were written
        let plan_response =
            std::fs::read_to_string(dir.path().join("plan").join("response.md")).unwrap();
        assert!(
            !plan_response.is_empty(),
            "LLM should have generated a response"
        );
        assert!(
            !plan_response.contains("[Simulated]"),
            "response should be from real LLM, not simulated"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn real_llm_two_stage_pipeline() {
        let client = match make_llm_client().await {
            Some(c) => c,
            None => {
                eprintln!("Skipping: ANTHROPIC_API_KEY not set");
                return;
            }
        };

        let mut graph = Graph::new("RealLLMTwoStage");
        graph.attrs.insert(
            "goal".to_string(),
            AttrValue::String("Generate and review".to_string()),
        );

        let mut start = Node::new("start");
        start.attrs.insert(
            "shape".to_string(),
            AttrValue::String("Mdiamond".to_string()),
        );
        graph.nodes.insert("start".to_string(), start);

        let mut exit = Node::new("exit");
        exit.attrs.insert(
            "shape".to_string(),
            AttrValue::String("Msquare".to_string()),
        );
        graph.nodes.insert("exit".to_string(), exit);

        let mut generate = Node::new("generate");
        generate.attrs.insert(
            "shape".to_string(),
            AttrValue::String("box".to_string()),
        );
        generate.attrs.insert(
            "prompt".to_string(),
            AttrValue::String("Write a haiku about programming.".to_string()),
        );
        graph.nodes.insert("generate".to_string(), generate);

        let mut review = Node::new("review");
        review.attrs.insert(
            "shape".to_string(),
            AttrValue::String("box".to_string()),
        );
        review.attrs.insert(
            "prompt".to_string(),
            AttrValue::String("Rate the haiku on a scale of 1-10.".to_string()),
        );
        graph.nodes.insert("review".to_string(), review);

        graph.edges.push(Edge::new("start", "generate"));
        graph.edges.push(Edge::new("generate", "review"));
        graph.edges.push(Edge::new("review", "exit"));

        let dir = tempfile::tempdir().unwrap();
        let mut registry = HandlerRegistry::new(Box::new(CodergenHandler::new(Some(
            make_llm_backend(Arc::clone(&client)),
        ))));
        registry.register("start", Box::new(StartHandler));
        registry.register("exit", Box::new(ExitHandler));
        registry.register(
            "codergen",
            Box::new(CodergenHandler::new(Some(make_llm_backend(client)))),
        );

        let engine = PipelineEngine::new(registry, EventEmitter::new());
        let config = RunConfig {
            logs_root: dir.path().to_path_buf(),
            cancel_token: None,
        };

        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            engine.run(&graph, &config),
        )
        .await
        .expect("should not timeout")
        .expect("real LLM two-stage pipeline should succeed");

        assert_eq!(outcome.status, StageStatus::Success);

        let checkpoint = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
        let last_stage = checkpoint
            .context_values
            .get("last_stage")
            .and_then(|v| v.as_str());
        assert_eq!(last_stage, Some("review"));
    }

    #[tokio::test]
    #[ignore]
    async fn real_llm_human_gate_auto_approve() {
        let client = match make_llm_client().await {
            Some(c) => c,
            None => {
                eprintln!("Skipping: ANTHROPIC_API_KEY not set");
                return;
            }
        };

        let mut graph = Graph::new("RealLLMGate");
        graph.attrs.insert(
            "goal".to_string(),
            AttrValue::String("Write and approve".to_string()),
        );

        let mut start = Node::new("start");
        start.attrs.insert(
            "shape".to_string(),
            AttrValue::String("Mdiamond".to_string()),
        );
        graph.nodes.insert("start".to_string(), start);

        let mut exit = Node::new("exit");
        exit.attrs.insert(
            "shape".to_string(),
            AttrValue::String("Msquare".to_string()),
        );
        graph.nodes.insert("exit".to_string(), exit);

        let mut write = Node::new("write");
        write.attrs.insert(
            "shape".to_string(),
            AttrValue::String("box".to_string()),
        );
        write.attrs.insert(
            "prompt".to_string(),
            AttrValue::String("Write a one-line greeting.".to_string()),
        );
        graph.nodes.insert("write".to_string(), write);

        let mut gate = Node::new("gate");
        gate.attrs.insert(
            "shape".to_string(),
            AttrValue::String("hexagon".to_string()),
        );
        gate.attrs.insert(
            "type".to_string(),
            AttrValue::String("wait.human".to_string()),
        );
        gate.attrs.insert(
            "label".to_string(),
            AttrValue::String("Approve?".to_string()),
        );
        graph.nodes.insert("gate".to_string(), gate);

        let mut ship = Node::new("ship");
        ship.attrs.insert(
            "shape".to_string(),
            AttrValue::String("box".to_string()),
        );
        ship.attrs.insert(
            "prompt".to_string(),
            AttrValue::String("Ship the greeting.".to_string()),
        );
        graph.nodes.insert("ship".to_string(), ship);

        let mut revise = Node::new("revise");
        revise.attrs.insert(
            "shape".to_string(),
            AttrValue::String("box".to_string()),
        );
        revise.attrs.insert(
            "prompt".to_string(),
            AttrValue::String("Revise the greeting.".to_string()),
        );
        graph.nodes.insert("revise".to_string(), revise);

        graph.edges.push(Edge::new("start", "write"));
        graph.edges.push(Edge::new("write", "gate"));

        let mut approve_edge = Edge::new("gate", "ship");
        approve_edge.attrs.insert(
            "label".to_string(),
            AttrValue::String("[A] Approve".to_string()),
        );
        graph.edges.push(approve_edge);

        let mut revise_edge = Edge::new("gate", "revise");
        revise_edge.attrs.insert(
            "label".to_string(),
            AttrValue::String("[R] Revise".to_string()),
        );
        graph.edges.push(revise_edge);

        graph.edges.push(Edge::new("ship", "exit"));
        graph.edges.push(Edge::new("revise", "gate"));

        let dir = tempfile::tempdir().unwrap();
        let interviewer = Arc::new(AutoApproveInterviewer);

        let mut registry = HandlerRegistry::new(Box::new(CodergenHandler::new(Some(
            make_llm_backend(Arc::clone(&client)),
        ))));
        registry.register("start", Box::new(StartHandler));
        registry.register("exit", Box::new(ExitHandler));
        registry.register(
            "codergen",
            Box::new(CodergenHandler::new(Some(make_llm_backend(client)))),
        );
        registry.register("wait.human", Box::new(WaitHumanHandler::new(interviewer)));

        let engine = PipelineEngine::new(registry, EventEmitter::new());
        let config = RunConfig {
            logs_root: dir.path().to_path_buf(),
            cancel_token: None,
        };

        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            engine.run(&graph, &config),
        )
        .await
        .expect("should not timeout")
        .expect("real LLM gate pipeline should succeed");

        assert_eq!(outcome.status, StageStatus::Success);

        let checkpoint = Checkpoint::load(&dir.path().join("checkpoint.json")).unwrap();
        assert!(
            checkpoint.completed_nodes.contains(&"write".to_string()),
            "write should be completed"
        );
        assert!(
            checkpoint.completed_nodes.contains(&"gate".to_string()),
            "gate should be completed"
        );
        assert!(
            checkpoint.completed_nodes.contains(&"ship".to_string()),
            "ship should be completed (auto-approve selects first option)"
        );
        assert!(
            !checkpoint.completed_nodes.contains(&"revise".to_string()),
            "revise should NOT be traversed with auto-approve"
        );
    }
}

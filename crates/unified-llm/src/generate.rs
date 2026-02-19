use crate::client::Client;
use crate::error::SdkError;
use crate::provider::StreamEventStream;
use crate::retry::retry;
use crate::tools::{execute_all_tools, Tool};
use crate::types::{
    FinishReason, GenerateResult, Message, Request, Response, ResponseFormat, ResponseFormatType,
    RetryPolicy, StepResult, StreamEvent, ToolCall, ToolChoice, ToolDefinition, Usage,
};
use std::sync::Arc;
use tokio::sync::OnceCell;

/// Module-level default client (Section 2.5).
static DEFAULT_CLIENT: OnceCell<Arc<Client>> = OnceCell::const_new();

/// Set the module-level default client.
pub fn set_default_client(client: Client) {
    let _ = DEFAULT_CLIENT.set(Arc::new(client));
}

/// Get the default client, lazily initialized from env.
fn get_default_client() -> Arc<Client> {
    DEFAULT_CLIENT
        .get()
        .cloned()
        .unwrap_or_else(|| Arc::new(Client::from_env()))
}

fn build_initial_messages(params: &GenerateParams) -> Result<Vec<Message>, SdkError> {
    let mut messages = Vec::new();
    if let Some(system) = &params.system {
        messages.push(Message::system(system));
    }
    if let Some(ref prompt) = params.prompt {
        if params.messages.is_some() {
            return Err(SdkError::Configuration {
                message: "Cannot specify both 'prompt' and 'messages'".into(),
            });
        }
        messages.push(Message::user(prompt));
    } else if let Some(ref msgs) = params.messages {
        messages.extend(msgs.clone());
    }
    Ok(messages)
}

fn build_request(
    params: &GenerateParams,
    messages: &[Message],
    tool_definitions: Option<&[ToolDefinition]>,
) -> Request {
    Request {
        model: params.model.clone(),
        messages: messages.to_vec(),
        provider: params.provider.clone(),
        tools: tool_definitions.map(<[ToolDefinition]>::to_vec),
        tool_choice: params.tool_choice.clone(),
        response_format: params.response_format.clone(),
        temperature: params.temperature,
        top_p: params.top_p,
        max_tokens: params.max_tokens,
        stop_sequences: params.stop_sequences.clone(),
        reasoning_effort: params.reasoning_effort.clone(),
        metadata: None,
        provider_options: params.provider_options.clone(),
    }
}

fn build_generate_result(steps: Vec<StepResult>, total_usage: Usage) -> GenerateResult {
    let last = steps.last().expect("steps should not be empty");
    let response = last.response.clone();
    let tool_results = last.tool_results.clone();
    GenerateResult {
        response,
        tool_results,
        total_usage,
        steps,
        output: None,
    }
}

/// High-level blocking generation function (Section 4.3).
///
/// Wraps `Client.complete()` with tool execution loops, prompt standardization,
/// and automatic retries.
///
/// # Errors
///
/// Returns `SdkError::Configuration` if both `prompt` and `messages` are set,
/// or any provider error encountered during generation or tool execution.
///
/// # Panics
///
/// Panics if a tool's `execute` handler is `None` when matched during tool execution.
pub async fn generate(params: GenerateParams) -> Result<GenerateResult, SdkError> {
    let client = params.client.clone().unwrap_or_else(get_default_client);
    let retry_policy = RetryPolicy {
        max_retries: params.max_retries,
        base_delay: 0.001,
        jitter: false,
        ..Default::default()
    };

    let mut messages = build_initial_messages(&params)?;
    let tool_definitions: Option<Vec<ToolDefinition>> = params
        .tools
        .as_ref()
        .map(|tools| tools.iter().map(|t| t.definition.clone()).collect());

    let max_tool_rounds = params.max_tool_rounds;
    let mut steps: Vec<StepResult> = Vec::new();
    let mut total_usage = Usage::default();

    let mut round = 0u32;
    loop {
        let request = build_request(&params, &messages, tool_definitions.as_deref());

        let client_ref = client.clone();
        let response = retry(&retry_policy, || {
            let c = client_ref.clone();
            let r = request.clone();
            async move { c.complete(&r).await }
        })
        .await?;

        let tool_calls = response.tool_calls();
        let mut tool_results = Vec::new();

        if !tool_calls.is_empty()
            && response.finish_reason == FinishReason::ToolCalls
            && params.tools.is_some()
        {
            let tools = params.tools.as_ref().expect("checked above");
            if tools.iter().any(|t| t.is_active()) {
                let tool_refs: Vec<&Tool> =
                    tools.iter().map(std::convert::AsRef::as_ref).collect();
                tool_results = execute_all_tools(&tool_refs, &tool_calls).await;
            }
        }

        total_usage = total_usage + response.usage.clone();

        let should_continue = !tool_calls.is_empty()
            && response.finish_reason == FinishReason::ToolCalls
            && round < max_tool_rounds
            && !tool_results.is_empty();

        if should_continue {
            messages.push(response.message.clone());
            for result in &tool_results {
                messages.push(Message::tool_result(
                    &result.tool_call_id,
                    result.content.to_string(),
                    result.is_error,
                ));
            }
        }

        steps.push(StepResult {
            response,
            tool_results,
        });

        if !should_continue {
            break;
        }

        round += 1;
    }

    Ok(build_generate_result(steps, total_usage))
}

/// Parameters for `generate()` (Section 4.3).
#[derive(Clone)]
pub struct GenerateParams {
    pub model: String,
    pub prompt: Option<String>,
    pub messages: Option<Vec<Message>>,
    pub system: Option<String>,
    pub tools: Option<Vec<Arc<Tool>>>,
    pub tool_choice: Option<ToolChoice>,
    pub max_tool_rounds: u32,
    pub response_format: Option<ResponseFormat>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub max_tokens: Option<i64>,
    pub stop_sequences: Option<Vec<String>>,
    pub reasoning_effort: Option<String>,
    pub provider: Option<String>,
    pub provider_options: Option<serde_json::Value>,
    pub max_retries: u32,
    pub client: Option<Arc<Client>>,
}

impl GenerateParams {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            prompt: None,
            messages: None,
            system: None,
            tools: None,
            tool_choice: None,
            max_tool_rounds: 1,
            response_format: None,
            temperature: None,
            top_p: None,
            max_tokens: None,
            stop_sequences: None,
            reasoning_effort: None,
            provider: None,
            provider_options: None,
            max_retries: 2,
            client: None,
        }
    }

    #[must_use]
    pub fn prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = Some(prompt.into());
        self
    }

    #[must_use]
    pub fn messages(mut self, messages: Vec<Message>) -> Self {
        self.messages = Some(messages);
        self
    }

    #[must_use]
    pub fn system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(system.into());
        self
    }

    #[must_use]
    pub fn client(mut self, client: Arc<Client>) -> Self {
        self.client = Some(client);
        self
    }

    #[must_use]
    pub fn tools(mut self, tools: Vec<Tool>) -> Self {
        self.tools = Some(tools.into_iter().map(Arc::new).collect());
        self
    }

    #[must_use]
    pub const fn max_tool_rounds(mut self, rounds: u32) -> Self {
        self.max_tool_rounds = rounds;
        self
    }

    #[must_use]
    pub fn provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = Some(provider.into());
        self
    }
}

/// `StreamAccumulator` collects stream events into a complete Response (Section 4.4).
pub struct StreamAccumulator {
    text_parts: Vec<String>,
    reasoning_parts: Vec<String>,
    tool_calls: Vec<ToolCall>,
    finish_reason: Option<FinishReason>,
    usage: Option<Usage>,
    response: Option<Response>,
}

impl StreamAccumulator {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            text_parts: Vec::new(),
            reasoning_parts: Vec::new(),
            tool_calls: Vec::new(),
            finish_reason: None,
            usage: None,
            response: None,
        }
    }

    pub fn process(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::TextDelta { delta, .. } => {
                self.text_parts.push(delta.clone());
            }
            StreamEvent::ReasoningDelta { delta } => {
                self.reasoning_parts.push(delta.clone());
            }
            StreamEvent::ToolCallEnd { tool_call } => {
                self.tool_calls.push(tool_call.clone());
            }
            StreamEvent::Finish {
                finish_reason,
                usage,
                response,
            } => {
                self.finish_reason = Some(finish_reason.clone());
                self.usage = Some(usage.clone());
                self.response = Some(*response.clone());
            }
            _ => {}
        }
    }

    #[must_use]
    pub const fn response(&self) -> Option<&Response> {
        self.response.as_ref()
    }

    #[must_use]
    pub fn text(&self) -> String {
        self.text_parts.join("")
    }

    #[must_use]
    pub fn reasoning(&self) -> Option<String> {
        if self.reasoning_parts.is_empty() {
            None
        } else {
            Some(self.reasoning_parts.join(""))
        }
    }
}

impl Default for StreamAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

/// High-level streaming generation (Section 4.4).
/// Returns a `StreamEventStream` that the caller can iterate over.
///
/// # Errors
///
/// Returns `SdkError::Configuration` if both `prompt` and `messages` are set,
/// or any provider error encountered during streaming setup.
pub async fn stream_generate(params: GenerateParams) -> Result<StreamEventStream, SdkError> {
    let client = params.client.clone().unwrap_or_else(get_default_client);
    let messages = build_initial_messages(&params)?;
    let tool_definitions: Option<Vec<ToolDefinition>> = params
        .tools
        .as_ref()
        .map(|tools| tools.iter().map(|t| t.definition.clone()).collect());

    let request = build_request(&params, &messages, tool_definitions.as_deref());
    client.stream(&request).await
}

/// Structured output generation with schema validation (Section 4.5).
///
/// # Errors
///
/// Returns `SdkError::NoObjectGenerated` if the response is not valid JSON,
/// or any error from `generate()`.
pub async fn generate_object(
    params: GenerateParams,
    schema: serde_json::Value,
) -> Result<GenerateResult, SdkError> {
    let params = GenerateParams {
        response_format: Some(ResponseFormat {
            kind: ResponseFormatType::JsonSchema,
            json_schema: Some(schema),
            strict: true,
        }),
        ..params
    };

    let mut result = generate(params).await?;

    // Try to parse the text as JSON
    match serde_json::from_str::<serde_json::Value>(&result.text()) {
        Ok(parsed) => {
            result.output = Some(parsed);
            Ok(result)
        }
        Err(e) => Err(SdkError::NoObjectGenerated {
            message: format!("Failed to parse response as JSON: {e}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::Client;
    use crate::provider::ProviderAdapter;
    use crate::types::{ContentPart, Role};
    use futures::stream;
    use futures::StreamExt;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Mock provider that returns configurable responses.
    struct MockProvider {
        response_text: String,
    }

    impl MockProvider {
        fn new(text: &str) -> Self {
            Self {
                response_text: text.to_string(),
            }
        }
    }

    #[async_trait::async_trait]
    impl ProviderAdapter for MockProvider {
        fn name(&self) -> &str {
            "mock"
        }

        async fn complete(&self, _request: &Request) -> Result<Response, SdkError> {
            Ok(Response {
                id: "resp_1".into(),
                model: "mock-model".into(),
                provider: "mock".into(),
                message: Message::assistant(&self.response_text),
                finish_reason: FinishReason::Stop,
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 20,
                    total_tokens: 30,
                    ..Default::default()
                },
                raw: None,
                warnings: vec![],
                rate_limit: None,
            })
        }

        async fn stream(
            &self,
            _request: &Request,
        ) -> Result<StreamEventStream, SdkError> {
            let text = self.response_text.clone();
            let events = vec![
                Ok(StreamEvent::text_delta(&text, Some("t1".into()))),
                Ok(StreamEvent::finish(
                    FinishReason::Stop,
                    Usage {
                        input_tokens: 10,
                        output_tokens: 20,
                        total_tokens: 30,
                        ..Default::default()
                    },
                    Response {
                        id: "resp_1".into(),
                        model: "mock-model".into(),
                        provider: "mock".into(),
                        message: Message::assistant(&text),
                        finish_reason: FinishReason::Stop,
                        usage: Usage {
                            input_tokens: 10,
                            output_tokens: 20,
                            total_tokens: 30,
                            ..Default::default()
                        },
                        raw: None,
                        warnings: vec![],
                        rate_limit: None,
                    },
                )),
            ];
            Ok(Box::pin(stream::iter(events)))
        }
    }

    fn mock_client(text: &str) -> Arc<Client> {
        let mut providers: HashMap<String, Arc<dyn ProviderAdapter>> = HashMap::new();
        providers.insert("mock".to_string(), Arc::new(MockProvider::new(text)));
        Arc::new(Client::new(
            providers,
            Some("mock".to_string()),
            vec![],
        ))
    }

    #[tokio::test]
    async fn generate_simple_text() {
        let result = generate(
            GenerateParams::new("mock-model")
                .prompt("Hello")
                .client(mock_client("Hi there!")),
        )
        .await
        .unwrap();

        assert_eq!(result.text(), "Hi there!");
        assert_eq!(*result.finish_reason(), FinishReason::Stop);
        assert_eq!(result.usage().input_tokens, 10);
        assert_eq!(result.steps.len(), 1);
    }

    #[tokio::test]
    async fn generate_with_system_message() {
        let result = generate(
            GenerateParams::new("mock-model")
                .system("You are helpful")
                .prompt("Hello")
                .client(mock_client("Greetings!")),
        )
        .await
        .unwrap();

        assert_eq!(result.text(), "Greetings!");
    }

    #[tokio::test]
    async fn generate_with_messages() {
        let result = generate(
            GenerateParams::new("mock-model")
                .messages(vec![
                    Message::user("Hello"),
                    Message::assistant("Hi"),
                    Message::user("How are you?"),
                ])
                .client(mock_client("I'm doing well!")),
        )
        .await
        .unwrap();

        assert_eq!(result.text(), "I'm doing well!");
    }

    #[tokio::test]
    async fn generate_errors_on_both_prompt_and_messages() {
        let result = generate(GenerateParams {
            model: "mock-model".into(),
            prompt: Some("Hello".into()),
            messages: Some(vec![Message::user("World")]),
            client: Some(mock_client("test")),
            ..GenerateParams::new("mock-model")
        })
        .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SdkError::Configuration { .. }
        ));
    }

    /// Mock provider that returns tool calls then text
    struct ToolCallMockProvider {
        call_count: Arc<AtomicU32>,
    }

    #[async_trait::async_trait]
    impl ProviderAdapter for ToolCallMockProvider {
        fn name(&self) -> &str {
            "mock"
        }

        async fn complete(&self, _request: &Request) -> Result<Response, SdkError> {
            let count = self.call_count.fetch_add(1, Ordering::SeqCst);

            if count == 0 {
                // First call: return tool call
                Ok(Response {
                    id: "resp_1".into(),
                    model: "mock-model".into(),
                    provider: "mock".into(),
                    message: Message {
                        role: Role::Assistant,
                        content: vec![ContentPart::ToolCall(ToolCall::new(
                            "call_1",
                            "get_weather",
                            serde_json::json!({"city": "SF"}),
                        ))],
                        name: None,
                        tool_call_id: None,
                    },
                    finish_reason: FinishReason::ToolCalls,
                    usage: Usage {
                        input_tokens: 10,
                        output_tokens: 5,
                        total_tokens: 15,
                        ..Default::default()
                    },
                    raw: None,
                    warnings: vec![],
                    rate_limit: None,
                })
            } else {
                // Second call: return text
                Ok(Response {
                    id: "resp_2".into(),
                    model: "mock-model".into(),
                    provider: "mock".into(),
                    message: Message::assistant("The weather in SF is 72F"),
                    finish_reason: FinishReason::Stop,
                    usage: Usage {
                        input_tokens: 20,
                        output_tokens: 10,
                        total_tokens: 30,
                        ..Default::default()
                    },
                    raw: None,
                    warnings: vec![],
                    rate_limit: None,
                })
            }
        }

        async fn stream(
            &self,
            _request: &Request,
        ) -> Result<StreamEventStream, SdkError> {
            Ok(Box::pin(stream::empty()))
        }
    }

    #[tokio::test]
    async fn generate_with_tool_loop() {
        let call_count = Arc::new(AtomicU32::new(0));
        let provider: Arc<dyn ProviderAdapter> = Arc::new(ToolCallMockProvider {
            call_count: call_count.clone(),
        });

        let mut providers: HashMap<String, Arc<dyn ProviderAdapter>> = HashMap::new();
        providers.insert("mock".to_string(), provider);
        let client = Arc::new(Client::new(
            providers,
            Some("mock".to_string()),
            vec![],
        ));

        let result = generate(
            GenerateParams::new("mock-model")
                .prompt("What's the weather in SF?")
                .tools(vec![Tool::active(
                    "get_weather",
                    "Get weather",
                    serde_json::json!({"type": "object", "properties": {"city": {"type": "string"}}}),
                    |args| async move {
                        let city = args["city"].as_str().unwrap_or("unknown");
                        Ok(serde_json::json!(format!("72F in {}", city)))
                    },
                )])
                .max_tool_rounds(5)
                .client(client),
        )
        .await
        .unwrap();

        assert_eq!(result.text(), "The weather in SF is 72F");
        assert_eq!(result.steps.len(), 2);
        assert_eq!(result.total_usage.input_tokens, 30);
        assert_eq!(result.total_usage.output_tokens, 15);
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn stream_accumulator_collects_events() {
        let mut acc = StreamAccumulator::new();

        acc.process(&StreamEvent::TextStart {
            text_id: Some("t1".into()),
        });

        acc.process(&StreamEvent::text_delta("Hello", Some("t1".into())));
        acc.process(&StreamEvent::text_delta(" world", Some("t1".into())));

        let resp = Response {
            id: "r1".into(),
            model: "m".into(),
            provider: "p".into(),
            message: Message::assistant("Hello world"),
            finish_reason: FinishReason::Stop,
            usage: Usage {
                input_tokens: 5,
                output_tokens: 2,
                total_tokens: 7,
                ..Default::default()
            },
            raw: None,
            warnings: vec![],
            rate_limit: None,
        };

        acc.process(&StreamEvent::finish(
            FinishReason::Stop,
            resp.usage.clone(),
            resp,
        ));

        assert_eq!(acc.text(), "Hello world");
        assert_eq!(acc.reasoning(), None);
        assert!(acc.response().is_some());
        assert_eq!(acc.response().unwrap().text(), "Hello world");
    }

    #[tokio::test]
    async fn stream_accumulator_collects_reasoning() {
        let mut acc = StreamAccumulator::new();

        acc.process(&StreamEvent::ReasoningDelta {
            delta: "Let me think...".into(),
        });

        assert_eq!(acc.reasoning(), Some("Let me think...".to_string()));
    }

    #[tokio::test]
    async fn stream_generate_returns_events() {
        let client = mock_client("Hello stream!");
        let mut stream = stream_generate(
            GenerateParams::new("mock-model")
                .prompt("Hi")
                .client(client),
        )
        .await
        .unwrap();

        let first = stream.next().await.unwrap().unwrap();
        match &first {
            StreamEvent::TextDelta { delta, .. } => assert_eq!(delta, "Hello stream!"),
            other => panic!("Expected TextDelta, got {other:?}"),
        }

        let second = stream.next().await.unwrap().unwrap();
        assert!(matches!(second, StreamEvent::Finish { .. }));
    }

    #[tokio::test]
    async fn generate_object_parses_json() {
        // Create a mock that returns valid JSON
        let client = mock_client(r#"{"name": "Alice", "age": 30}"#);

        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer"}
            },
            "required": ["name", "age"]
        });

        let result = generate_object(
            GenerateParams::new("mock-model")
                .prompt("Extract name and age")
                .client(client),
            schema,
        )
        .await
        .unwrap();

        assert!(result.output.is_some());
        let output = result.output.unwrap();
        assert_eq!(output["name"], "Alice");
        assert_eq!(output["age"], 30);
    }

    #[tokio::test]
    async fn generate_object_errors_on_invalid_json() {
        let client = mock_client("not valid json");

        let result = generate_object(
            GenerateParams::new("mock-model")
                .prompt("Extract data")
                .client(client),
            serde_json::json!({"type": "object"}),
        )
        .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SdkError::NoObjectGenerated { .. }
        ));
    }
}

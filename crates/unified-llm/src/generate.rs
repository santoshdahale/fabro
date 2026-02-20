use crate::client::Client;
use crate::error::SdkError;
use crate::provider::StreamEventStream;
use crate::retry::retry;
use crate::tools::{execute_all_tools, Tool};
use crate::types::{
    FinishReason, GenerateResult, Message, ObjectStreamEvent, Request, Response, ResponseFormat,
    ResponseFormatType, RetryPolicy, StepResult, StreamEvent, TimeoutConfig, ToolCall, ToolChoice,
    ToolDefinition, Usage,
};
use futures::StreamExt;
use std::pin::Pin;
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
        metadata: params.metadata.clone(),
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

    let generate_future = async {
        let mut steps: Vec<StepResult> = Vec::new();
        let mut total_usage = Usage::default();

        let mut round = 0u32;
        loop {
            let request = build_request(&params, &messages, tool_definitions.as_deref());

            let client_ref = client.clone();
            let response =
                if let Some(per_step) = params.timeout.as_ref().and_then(|t| t.per_step) {
                    let duration = std::time::Duration::from_secs_f64(per_step);
                    tokio::time::timeout(duration, retry(&retry_policy, || {
                        let c = client_ref.clone();
                        let r = request.clone();
                        async move { c.complete(&r).await }
                    }))
                    .await
                    .map_err(|_| SdkError::RequestTimeout {
                        message: format!("Per-step timeout of {per_step}s exceeded"),
                    })?
                } else {
                    retry(&retry_policy, || {
                        let c = client_ref.clone();
                        let r = request.clone();
                        async move { c.complete(&r).await }
                    })
                    .await
                }?;

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

            steps.push(StepResult {
                response,
                tool_results,
            });

            let last = steps.last().expect("just pushed");
            let should_continue = !tool_calls.is_empty()
                && last.response.finish_reason == FinishReason::ToolCalls
                && round < max_tool_rounds
                && !last.tool_results.is_empty()
                && !params.stop_when.as_ref().is_some_and(|f| f(&steps));

            if !should_continue {
                break;
            }

            let last = steps.last().expect("just pushed");
            messages.push(last.response.message.clone());
            for result in &last.tool_results {
                messages.push(Message::tool_result(
                    &result.tool_call_id,
                    result.content.to_string(),
                    result.is_error,
                ));
            }

            round += 1;
        }

        Ok(build_generate_result(steps, total_usage))
    };

    if let Some(total) = params.timeout.as_ref().and_then(|t| t.total) {
        let duration = std::time::Duration::from_secs_f64(total);
        tokio::time::timeout(duration, generate_future)
            .await
            .map_err(|_| SdkError::RequestTimeout {
                message: format!("Total timeout of {total}s exceeded"),
            })?
    } else {
        generate_future.await
    }
}

/// Callback type for custom stop conditions in the tool loop.
pub type StopCondition = Arc<dyn Fn(&[StepResult]) -> bool + Send + Sync>;

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
    pub metadata: Option<std::collections::HashMap<String, String>>,
    pub max_retries: u32,
    pub timeout: Option<TimeoutConfig>,
    pub client: Option<Arc<Client>>,
    /// Custom stop condition checked after each tool round (Section 4.3).
    pub stop_when: Option<StopCondition>,
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
            metadata: None,
            max_retries: 2,
            timeout: None,
            client: None,
            stop_when: None,
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

    #[must_use]
    pub fn tool_choice(mut self, tool_choice: ToolChoice) -> Self {
        self.tool_choice = Some(tool_choice);
        self
    }

    #[must_use]
    pub fn response_format(mut self, response_format: ResponseFormat) -> Self {
        self.response_format = Some(response_format);
        self
    }

    #[must_use]
    pub const fn temperature(mut self, temperature: f64) -> Self {
        self.temperature = Some(temperature);
        self
    }

    #[must_use]
    pub const fn top_p(mut self, top_p: f64) -> Self {
        self.top_p = Some(top_p);
        self
    }

    #[must_use]
    pub const fn max_tokens(mut self, max_tokens: i64) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    #[must_use]
    pub fn stop_sequences(mut self, stop_sequences: Vec<String>) -> Self {
        self.stop_sequences = Some(stop_sequences);
        self
    }

    #[must_use]
    pub fn reasoning_effort(mut self, reasoning_effort: impl Into<String>) -> Self {
        self.reasoning_effort = Some(reasoning_effort.into());
        self
    }

    #[must_use]
    pub fn provider_options(mut self, provider_options: serde_json::Value) -> Self {
        self.provider_options = Some(provider_options);
        self
    }

    #[must_use]
    pub fn metadata(mut self, metadata: std::collections::HashMap<String, String>) -> Self {
        self.metadata = Some(metadata);
        self
    }

    #[must_use]
    pub const fn max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    #[must_use]
    pub const fn timeout(mut self, timeout: TimeoutConfig) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set a custom stop condition for the tool loop (Section 4.3).
    ///
    /// The callback receives the accumulated steps so far and returns `true`
    /// to stop the tool loop early.
    #[must_use]
    pub fn stop_when(
        mut self,
        f: impl Fn(&[StepResult]) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.stop_when = Some(Arc::new(f));
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
pub async fn stream(params: GenerateParams) -> Result<StreamEventStream, SdkError> {
    stream_generate(params).await
}

/// High-level streaming generation (Section 4.4).
/// Returns a `StreamEventStream` that the caller can iterate over.
///
/// Alias: prefer [`stream()`] for consistency with the spec.
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

/// Stream type for `stream_object()`.
pub type ObjectStream =
    Pin<Box<dyn futures::Stream<Item = Result<ObjectStreamEvent, SdkError>> + Send>>;

/// Streaming structured output with incremental JSON parsing (Section 4.6).
///
/// Combines streaming with structured output: sets `response_format` to `json_schema`,
/// streams the response, and attempts to parse the accumulated text as JSON on each
/// text delta. Yields `ObjectStreamEvent::Partial` when a new valid partial parse is
/// obtained, `ObjectStreamEvent::Delta` for every raw stream event, and
/// `ObjectStreamEvent::Complete` when the stream finishes with the final parsed object.
///
/// # Errors
///
/// Returns `SdkError::Configuration` if both `prompt` and `messages` are set,
/// `SdkError::NoObjectGenerated` if the final accumulated text is not valid JSON,
/// or any provider error encountered during streaming.
pub async fn stream_object(
    params: GenerateParams,
    schema: serde_json::Value,
) -> Result<ObjectStream, SdkError> {
    let params = GenerateParams {
        response_format: Some(ResponseFormat {
            kind: ResponseFormatType::JsonSchema,
            json_schema: Some(schema),
            strict: true,
        }),
        ..params
    };

    let inner_stream = stream(params).await?;

    let mapped = inner_stream.scan(
        (String::new(), Option::<serde_json::Value>::None),
        |(accumulated_text, last_parsed), event| {
            let mut events: Vec<Result<ObjectStreamEvent, SdkError>> = Vec::new();

            match &event {
                Ok(stream_event) => {
                    // Accumulate text from TextDelta events
                    if let StreamEvent::TextDelta { delta, .. } = stream_event {
                        accumulated_text.push_str(delta);

                        // Try incremental JSON parse
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(accumulated_text) {
                            if last_parsed.as_ref() != Some(&parsed) {
                                *last_parsed = Some(parsed.clone());
                                events.push(Ok(ObjectStreamEvent::Partial { object: parsed }));
                            }
                        }
                    }

                    // On Finish, yield the Complete event with final parsed object
                    if let StreamEvent::Finish { response, .. } = stream_event {
                        match serde_json::from_str::<serde_json::Value>(accumulated_text) {
                            Ok(final_object) => {
                                events.push(Ok(ObjectStreamEvent::Complete {
                                    object: final_object,
                                    response: response.clone(),
                                }));
                            }
                            Err(e) => {
                                events.push(Err(SdkError::NoObjectGenerated {
                                    message: format!("Failed to parse final response as JSON: {e}"),
                                }));
                            }
                        }
                    } else {
                        // Yield the raw delta event
                        events.push(Ok(ObjectStreamEvent::Delta {
                            event: stream_event.clone(),
                        }));
                    }
                }
                Err(e) => {
                    events.push(Err(SdkError::Stream {
                        message: format!("{e}"),
                    }));
                }
            }

            futures::future::ready(Some(futures::stream::iter(events)))
        },
    );

    Ok(Box::pin(mapped.flatten()))
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

    #[tokio::test]
    async fn generate_stop_when_halts_tool_loop() {
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
                .stop_when(|_steps| true) // Stop immediately after first round
                .client(client),
        )
        .await
        .unwrap();

        // stop_when returned true, so the tool loop should stop after 1 step
        assert_eq!(result.steps.len(), 1);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn generate_params_builder_methods() {
        let params = GenerateParams::new("test-model")
            .prompt("hello")
            .system("you are helpful")
            .temperature(0.7)
            .top_p(0.9)
            .max_tokens(100)
            .stop_sequences(vec!["STOP".to_string()])
            .reasoning_effort("high")
            .provider("anthropic")
            .provider_options(serde_json::json!({"key": "value"}))
            .max_retries(5)
            .tool_choice(ToolChoice::Required)
            .response_format(ResponseFormat {
                kind: ResponseFormatType::JsonObject,
                json_schema: None,
                strict: false,
            })
            .max_tool_rounds(3);

        assert_eq!(params.model, "test-model");
        assert_eq!(params.prompt.as_deref(), Some("hello"));
        assert_eq!(params.system.as_deref(), Some("you are helpful"));
        assert_eq!(params.temperature, Some(0.7));
        assert_eq!(params.top_p, Some(0.9));
        assert_eq!(params.max_tokens, Some(100));
        assert_eq!(
            params.stop_sequences,
            Some(vec!["STOP".to_string()])
        );
        assert_eq!(params.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(params.provider.as_deref(), Some("anthropic"));
        assert!(params.provider_options.is_some());
        assert_eq!(params.max_retries, 5);
        assert_eq!(params.tool_choice, Some(ToolChoice::Required));
        assert!(params.response_format.is_some());
        assert_eq!(params.max_tool_rounds, 3);
    }

    #[test]
    fn generate_params_timeout_builder() {
        let params = GenerateParams::new("test-model")
            .timeout(TimeoutConfig {
                total: Some(30.0),
                per_step: Some(10.0),
            });
        assert!(params.timeout.is_some());
        let t = params.timeout.unwrap();
        assert_eq!(t.total, Some(30.0));
        assert_eq!(t.per_step, Some(10.0));
    }

    /// Mock provider that streams JSON tokens incrementally.
    struct StreamingJsonMockProvider {
        deltas: Vec<String>,
        full_text: String,
    }

    impl StreamingJsonMockProvider {
        fn new(deltas: Vec<&str>) -> Self {
            let full_text: String = deltas.iter().copied().collect();
            Self {
                deltas: deltas.into_iter().map(String::from).collect(),
                full_text,
            }
        }
    }

    #[async_trait::async_trait]
    impl ProviderAdapter for StreamingJsonMockProvider {
        fn name(&self) -> &str {
            "mock"
        }

        async fn complete(&self, _request: &Request) -> Result<Response, SdkError> {
            Ok(Response {
                id: "resp_1".into(),
                model: "mock-model".into(),
                provider: "mock".into(),
                message: Message::assistant(&self.full_text),
                finish_reason: FinishReason::Stop,
                usage: Usage::default(),
                raw: None,
                warnings: vec![],
                rate_limit: None,
            })
        }

        async fn stream(
            &self,
            _request: &Request,
        ) -> Result<StreamEventStream, SdkError> {
            let mut events: Vec<Result<StreamEvent, SdkError>> = self
                .deltas
                .iter()
                .map(|d| Ok(StreamEvent::text_delta(d.as_str(), Some("t1".into()))))
                .collect();

            events.push(Ok(StreamEvent::finish(
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
                    message: Message::assistant(&self.full_text),
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
            )));

            Ok(Box::pin(stream::iter(events)))
        }
    }

    fn streaming_json_mock_client(deltas: Vec<&str>) -> Arc<Client> {
        let mut providers: HashMap<String, Arc<dyn ProviderAdapter>> = HashMap::new();
        providers.insert(
            "mock".to_string(),
            Arc::new(StreamingJsonMockProvider::new(deltas)),
        );
        Arc::new(Client::new(
            providers,
            Some("mock".to_string()),
            vec![],
        ))
    }

    #[tokio::test]
    async fn stream_object_yields_complete_event() {
        let client = streaming_json_mock_client(vec![r#"{"name": "Alice", "age": 30}"#]);

        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer"}
            },
            "required": ["name", "age"]
        });

        let obj_stream = stream_object(
            GenerateParams::new("mock-model")
                .prompt("Extract info")
                .client(client),
            schema,
        )
        .await
        .unwrap();

        let events: Vec<ObjectStreamEvent> = obj_stream
            .filter_map(|r| futures::future::ready(r.ok()))
            .collect()
            .await;

        let complete = events
            .iter()
            .find(|e| matches!(e, ObjectStreamEvent::Complete { .. }));
        assert!(complete.is_some(), "Expected a Complete event");

        if let ObjectStreamEvent::Complete { object, .. } = complete.unwrap() {
            assert_eq!(object["name"], "Alice");
            assert_eq!(object["age"], 30);
        }
    }

    #[tokio::test]
    async fn stream_object_yields_partial_events_incrementally() {
        let client = streaming_json_mock_client(vec![
            r#"{"name""#,
            r#": "Bob""#,
            r#", "age": 25}"#,
        ]);

        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer"}
            }
        });

        let obj_stream = stream_object(
            GenerateParams::new("mock-model")
                .prompt("Extract info")
                .client(client),
            schema,
        )
        .await
        .unwrap();

        let events: Vec<ObjectStreamEvent> = obj_stream
            .filter_map(|r| futures::future::ready(r.ok()))
            .collect()
            .await;

        let partial_count = events
            .iter()
            .filter(|e| matches!(e, ObjectStreamEvent::Partial { .. }))
            .count();

        assert!(
            partial_count >= 1,
            "Expected at least one Partial event, got {partial_count}"
        );

        let delta_count = events
            .iter()
            .filter(|e| matches!(e, ObjectStreamEvent::Delta { .. }))
            .count();

        assert_eq!(delta_count, 3);

        let last_complete = events
            .iter()
            .rev()
            .find(|e| matches!(e, ObjectStreamEvent::Complete { .. }));
        assert!(last_complete.is_some(), "Expected a Complete event");
        if let ObjectStreamEvent::Complete { object, .. } = last_complete.unwrap() {
            assert_eq!(object["name"], "Bob");
            assert_eq!(object["age"], 25);
        }
    }

    #[tokio::test]
    async fn stream_object_errors_on_invalid_final_json() {
        let client = streaming_json_mock_client(vec![r#"{"name": "Alice"#]);

        let schema = serde_json::json!({"type": "object"});

        let obj_stream = stream_object(
            GenerateParams::new("mock-model")
                .prompt("Extract info")
                .client(client),
            schema,
        )
        .await
        .unwrap();

        let results: Vec<Result<ObjectStreamEvent, SdkError>> = obj_stream.collect().await;

        let has_error = results.iter().any(|r| r.is_err());
        assert!(has_error, "Expected an error for invalid final JSON");
    }
}

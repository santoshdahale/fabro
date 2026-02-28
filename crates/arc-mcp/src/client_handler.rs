use rmcp::model::{
    CancelledNotificationParam, LoggingLevel, LoggingMessageNotificationParam,
    ProgressNotificationParam, ResourceUpdatedNotificationParam,
};
use rmcp::service::NotificationContext;
use rmcp::{ClientHandler, RoleClient};
use tracing::{debug, error, info, warn};

/// Minimal MCP client handler that logs server notifications via tracing.
#[derive(Clone)]
pub(crate) struct LoggingClientHandler;

impl ClientHandler for LoggingClientHandler {
    fn get_info(&self) -> rmcp::model::ClientInfo {
        rmcp::model::ClientInfo {
            protocol_version: rmcp::model::ProtocolVersion::V_2025_03_26,
            capabilities: rmcp::model::ClientCapabilities::default(),
            client_info: rmcp::model::Implementation {
                name: "arc-mcp".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                title: None,
                description: None,
                icons: None,
                website_url: None,
            },
            meta: None,
        }
    }

    async fn on_cancelled(
        &self,
        params: CancelledNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        info!(
            request_id = %params.request_id,
            reason = ?params.reason,
            "MCP server cancelled request"
        );
    }

    async fn on_progress(
        &self,
        params: ProgressNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        debug!(
            progress_token = ?params.progress_token,
            progress = params.progress,
            total = ?params.total,
            message = ?params.message,
            "MCP server progress"
        );
    }

    async fn on_resource_updated(
        &self,
        params: ResourceUpdatedNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        info!(uri = %params.uri, "MCP server resource updated");
    }

    async fn on_resource_list_changed(&self, _context: NotificationContext<RoleClient>) {
        info!("MCP server resource list changed");
    }

    async fn on_tool_list_changed(&self, _context: NotificationContext<RoleClient>) {
        info!("MCP server tool list changed");
    }

    async fn on_prompt_list_changed(&self, _context: NotificationContext<RoleClient>) {
        info!("MCP server prompt list changed");
    }

    async fn on_logging_message(
        &self,
        params: LoggingMessageNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        let logger = params.logger.as_deref();
        match params.level {
            LoggingLevel::Emergency
            | LoggingLevel::Alert
            | LoggingLevel::Critical
            | LoggingLevel::Error => {
                error!(level = ?params.level, ?logger, data = %params.data, "MCP server log");
            }
            LoggingLevel::Warning => {
                warn!(level = ?params.level, ?logger, data = %params.data, "MCP server log");
            }
            LoggingLevel::Notice | LoggingLevel::Info => {
                info!(level = ?params.level, ?logger, data = %params.data, "MCP server log");
            }
            LoggingLevel::Debug => {
                debug!(level = ?params.level, ?logger, data = %params.data, "MCP server log");
            }
        }
    }
}

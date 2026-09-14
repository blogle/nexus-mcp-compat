use std::{borrow::Cow, fmt::Display, sync::Arc};

use rmcp::{
    ClientLifecycleMode, ClientServiceExt, ErrorData, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, ClientCapabilities, ClientInfo, Implementation,
        ListToolsResult, PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerInfo,
    },
    service::{RequestContext, RoleClient, RoleServer, RunningService},
    transport::StreamableHttpClientTransport,
};

const CLIENT_NAME: &str = "nexus-mcp-compat";

#[derive(Clone, Debug)]
pub struct NexusCompat {
    nexus_mcp_url: Arc<str>,
}

impl NexusCompat {
    pub fn new(nexus_mcp_url: impl Into<Arc<str>>) -> Self {
        Self {
            nexus_mcp_url: nexus_mcp_url.into(),
        }
    }

    async fn connect_legacy(&self) -> Result<RunningService<RoleClient, ClientInfo>, ErrorData> {
        let transport = StreamableHttpClientTransport::from_uri(self.nexus_mcp_url.clone());

        let client = ClientInfo::new(
            ClientCapabilities::builder().build(),
            Implementation::new(CLIENT_NAME, env!("CARGO_PKG_VERSION")),
        )
        .with_protocol_version(ProtocolVersion::V_2025_06_18);

        client
            .serve_with_lifecycle(transport, ClientLifecycleMode::Initialize)
            .await
            .map_err(|error| upstream_error("initialize", error))
    }
}

impl ServerHandler for NexusCompat {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
        .with_protocol_version(ProtocolVersion::V_2026_07_28)
        .with_server_info(Implementation::new(
            CLIENT_NAME,
            env!("CARGO_PKG_VERSION"),
        ))
        .with_instructions(
            "Compatibility bridge exposing Nexus MCP tools through the MCP 2026-07-28 protocol.",
        )
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Owned(vec![ProtocolVersion::V_2026_07_28])
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let client = self.connect_legacy().await?;

        let result = client
            .list_tools(request)
            .await
            .map_err(|error| upstream_error("tools/list", error));

        if let Err(error) = client.cancel().await {
            tracing::debug!(
                error = %error,
                "error while closing Nexus client after tools/list"
            );
        }

        result
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let tool_name = request.name.to_string();

        let client = self.connect_legacy().await?;

        tracing::debug!(
            tool = %tool_name,
            "forwarding tools/call to Nexus"
        );

        let result = client
            .call_tool_once(request)
            .await
            .map_err(|error| upstream_error("tools/call", error));

        if let Err(error) = client.cancel().await {
            tracing::debug!(
                error = %error,
                "error while closing Nexus client after tools/call"
            );
        }

        result
    }
}

fn upstream_error(stage: &'static str, error: impl Display) -> ErrorData {
    tracing::warn!(
        stage,
        error = %error,
        "Nexus MCP request failed"
    );

    ErrorData::internal_error(format!("Nexus upstream {stage} failed: {error}"), None)
}

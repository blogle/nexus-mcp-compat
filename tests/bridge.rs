use std::sync::Arc;

use axum::{
    Json, Router,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use nexus_mcp_compat::NexusCompat;
use rmcp::{
    ClientLifecycleMode, ClientServiceExt,
    model::{
        CallToolRequestParams, CallToolResponse, ClientCapabilities, ClientInfo, Implementation,
        ProtocolVersion,
    },
    transport::{
        StreamableHttpClientTransport,
        streamable_http_server::{
            StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
        },
    },
};
use serde_json::{Value, json};
use tokio::net::TcpListener;

async fn fake_legacy_nexus(Json(body): Json<Value>) -> Response {
    let method = body
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();

    match method {
        "initialize" => {
            let id = body.get("id").cloned().unwrap_or(Value::Null);

            Json(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "fake-nexus",
                        "version": "0.0.0"
                    }
                }
            }))
            .into_response()
        }

        "notifications/initialized" => StatusCode::ACCEPTED.into_response(),

        "tools/list" => {
            let id = body.get("id").cloned().unwrap_or(Value::Null);

            Json(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tools": [
                        {
                            "name": "search",
                            "description": "Search the Nexus tool index",
                            "inputSchema": {
                                "type": "object",
                                "additionalProperties": true
                            }
                        },
                        {
                            "name": "execute",
                            "description": "Execute a Nexus-discovered tool",
                            "inputSchema": {
                                "type": "object",
                                "additionalProperties": true
                            }
                        }
                    ]
                }
            }))
            .into_response()
        }

        "tools/call" => {
            let id = body.get("id").cloned().unwrap_or(Value::Null);

            let name = body
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or("unknown");

            Json(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [
                        {
                            "type": "text",
                            "text": format!("fake nexus called {name}")
                        }
                    ],
                    "isError": false
                }
            }))
            .into_response()
        }

        other => {
            let id = body.get("id").cloned().unwrap_or(Value::Null);

            Json(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": format!("method not found: {other}")
                }
            }))
            .into_response()
        }
    }
}

async fn start_fake_nexus() -> std::net::SocketAddr {
    let app = Router::new().route("/mcp", post(fake_legacy_nexus));

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake Nexus");

    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("fake Nexus server failed");
    });

    addr
}

async fn start_bridge(nexus_url: String) -> std::net::SocketAddr {
    let bridge = NexusCompat::new(Arc::<str>::from(nexus_url));

    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .disable_allowed_hosts();

    let service = StreamableHttpService::new(
        move || Ok::<_, std::io::Error>(bridge.clone()),
        LocalSessionManager::default().into(),
        config,
    );

    let app = Router::new().nest_service("/mcp", service);

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind compatibility bridge");

    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("compatibility bridge failed");
    });

    addr
}

#[tokio::test]
async fn modern_client_can_discover_and_use_legacy_nexus() {
    let nexus_addr = start_fake_nexus().await;

    let bridge_addr = start_bridge(format!("http://{nexus_addr}/mcp")).await;

    let transport = StreamableHttpClientTransport::from_uri(format!("http://{bridge_addr}/mcp"));

    let client = ClientInfo::new(
        ClientCapabilities::builder().build(),
        Implementation::new("bridge-test", "0.0.0"),
    )
    .with_protocol_version(ProtocolVersion::V_2026_07_28)
    .serve_with_lifecycle(
        transport,
        ClientLifecycleMode::Discover {
            preferred_versions: vec![ProtocolVersion::V_2026_07_28],
        },
    )
    .await
    .expect("2026-07-28 server/discover must succeed");

    let tools = client
        .list_tools(None)
        .await
        .expect("tools/list must bridge to legacy Nexus");

    let mut names = tools
        .tools
        .iter()
        .map(|tool| tool.name.to_string())
        .collect::<Vec<_>>();

    names.sort();

    assert_eq!(names, vec!["execute".to_string(), "search".to_string()]);

    let response = client
        .call_tool_once(CallToolRequestParams::new("search"))
        .await
        .expect("tools/call must bridge to legacy Nexus");

    match response {
        CallToolResponse::Complete(result) => {
            let debug = format!("{result:?}");

            assert!(
                debug.contains("fake nexus called search"),
                "unexpected call result: {debug}"
            );
        }

        other => {
            panic!("unexpected tools/call response: {other:?}");
        }
    }

    client.cancel().await.unwrap();
}

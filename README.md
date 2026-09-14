# nexus-mcp-compat

A deliberately small MCP protocol compatibility shim.

It exposes a modern MCP `2026-07-28` Streamable HTTP endpoint and forwards
`tools/list` and `tools/call` to a legacy Nexus MCP endpoint using the
`2025-06-18` initialization lifecycle.

## Why

Nexus 0.6.0 speaks MCP `2025-06-18`. ChatGPT Developer Mode currently reaches
custom MCP servers using MCP `2026-07-28`, beginning with `server/discover`.

This service bridges that protocol boundary without introducing another MCP
router.

Nexus remains responsible for tool aggregation, search, execution, policy, and
downstream MCP servers.

## Architecture

    ChatGPT
        |
    OpenAI Secure MCP Tunnel
        |
        | MCP 2026-07-28
        v
    nexus-mcp-compat
        |
        | MCP 2025-06-18
        v
    Nexus
        |
        +-- search
        +-- execute

## Scope

The shim exposes only the MCP tools capability.

Incoming `server/discover` is handled by the official Rust MCP SDK.

`tools/list` and `tools/call` are forwarded to Nexus.

No resource, prompt, sampling, elicitation, authentication, routing, policy,
registry, or tool aggregation functionality is implemented here.

## Configuration

Required:

    NEXUS_MCP_URL=http://nexus.nexus.svc.cluster.local:8000/mcp

Optional:

    LISTEN_ADDR=0.0.0.0:8000
    RUST_LOG=info
    MCP_ALLOWED_HOSTS=localhost,127.0.0.1,nexus-mcp-compat,nexus-mcp-compat.nexus.svc,nexus-mcp-compat.nexus.svc.cluster.local

Endpoints:

    POST /mcp
    GET  /healthz
    GET  /readyz

## Development

Enter the Crane-provided environment:

    nix develop

Run checks:

    nix flake check -L

Run locally:

    NEXUS_MCP_URL=http://127.0.0.1:8001/mcp cargo run

Build the static package:

    nix build

Build the OCI image:

    nix build .#oci

## Protocol smoke test

A modern discovery request should return HTTP 200 and advertise only
`2026-07-28`.

    curl -i \
      -X POST \
      -H 'Content-Type: application/json' \
      -H 'Accept: application/json, text/event-stream' \
      -H 'MCP-Protocol-Version: 2026-07-28' \
      -H 'Mcp-Method: server/discover' \
      --data '{
        "jsonrpc":"2.0",
        "id":"discover-1",
        "method":"server/discover",
        "params":{
          "_meta":{
            "io.modelcontextprotocol/protocolVersion":"2026-07-28",
            "io.modelcontextprotocol/clientInfo":{
              "name":"smoke-test",
              "version":"1.0.0"
            },
            "io.modelcontextprotocol/clientCapabilities":{}
          }
        }
      }' \
      http://127.0.0.1:8000/mcp

## Container

GitHub Actions publishes:

    ghcr.io/blogle/nexus-mcp-compat:latest

and an immutable commit tag:

    ghcr.io/blogle/nexus-mcp-compat:sha-<12-character-sha>

The image is built from a statically linked musl binary and contains no shell
or general-purpose userspace.

## Non-goals

This is not an MCP gateway.

If Nexus gains MCP `2026-07-28` support, this service should be deleted and the
OpenAI tunnel should be pointed directly back at Nexus.

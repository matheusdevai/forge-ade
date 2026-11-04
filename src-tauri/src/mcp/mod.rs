//! MCP Engine: server registry (stdio / Streamable HTTP), secrets in the OS
//! keychain, a minimal MCP client to verify servers and list their tools, and
//! per-agent attachment (Claude Code `--mcp-config` + `--strict-mcp-config`).

pub mod client;
pub mod repo;
pub mod secrets;

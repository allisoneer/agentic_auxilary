#!/bin/bash
# Simple test script to verify MCP server basic functionality with complete handshake

# Send initialize, then initialized notification, then tools/list
{
  echo '{"jsonrpc":"2.0","method":"initialize","params":{"protocolVersion":"1.0","clientCapabilities":{}},"id":1}'
  echo '{"jsonrpc":"2.0","method":"notifications/initialized"}'
  echo '{"jsonrpc":"2.0","method":"tools/list","params":{},"id":2}'
} | cargo run -p example-05-mcp-basic 2>&1 | head -50
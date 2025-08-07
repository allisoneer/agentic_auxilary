#!/bin/bash
# Simple test script to verify MCP server basic functionality

# Send initialize request
echo '{"jsonrpc":"2.0","method":"initialize","params":{"protocolVersion":"1.0","clientCapabilities":{}},"id":1}' | cargo run -p example-05-mcp-basic 2>&1 | head -20
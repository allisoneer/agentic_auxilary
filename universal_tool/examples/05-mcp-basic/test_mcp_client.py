#!/usr/bin/env python3
"""
Simple MCP test client for testing the UTF MCP server example.

This script connects to the MCP server and tests the available tools.
It uses JSON-RPC over stdio to communicate with the server.
"""

import json
import subprocess
import sys
from typing import Any, Dict, Optional

class McpTestClient:
    def __init__(self, server_command: list[str]):
        """Initialize the MCP test client with a server command."""
        self.server_command = server_command
        self.process: Optional[subprocess.Popen] = None
        self.request_id = 0
        
    def start_server(self):
        """Start the MCP server process."""
        self.process = subprocess.Popen(
            self.server_command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=0
        )
        print(f"Started MCP server with PID: {self.process.pid}")
        
    def send_request(self, method: str, params: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
        """Send a JSON-RPC request to the server."""
        if not self.process:
            raise RuntimeError("Server not started")
            
        self.request_id += 1
        request = {
            "jsonrpc": "2.0",
            "method": method,
            "id": self.request_id
        }
        
        if params is not None:
            request["params"] = params
            
        # Send request
        request_str = json.dumps(request) + "\n"
        print(f"\n→ Sending: {request_str.strip()}")
        self.process.stdin.write(request_str)
        self.process.stdin.flush()
        
        # Read response
        response_line = self.process.stdout.readline()
        if not response_line:
            # Check if process has terminated
            if self.process.poll() is not None:
                stderr = self.process.stderr.read()
                raise RuntimeError(f"Server terminated unexpectedly. Stderr: {stderr}")
            raise RuntimeError("No response from server")
            
        print(f"← Received: {response_line.strip()}")
        
        try:
            response = json.loads(response_line)
            return response
        except json.JSONDecodeError as e:
            raise RuntimeError(f"Invalid JSON response: {response_line}") from e
            
    def close(self):
        """Close the server process."""
        if self.process:
            self.process.terminate()
            self.process.wait()
            print(f"\nServer terminated with code: {self.process.returncode}")
            
def main():
    """Test the MCP server functionality."""
    # Command to run the server
    server_cmd = ["cargo", "run", "--example", "05-mcp-basic"]
    
    client = McpTestClient(server_cmd)
    
    try:
        print("🧪 MCP Server Test Client")
        print("=" * 60)
        
        # Start the server
        client.start_server()
        
        # Wait a moment for server to start
        import time
        time.sleep(0.5)
        
        # Test 1: Initialize
        print("\n1. Testing initialize...")
        response = client.send_request("initialize", {
            "protocolVersion": "1.0",
            "clientCapabilities": {}
        })
        
        if "result" in response:
            print("✅ Initialize successful!")
            print(f"   Server: {response['result'].get('serverInfo', {})}")
            print(f"   Capabilities: {response['result'].get('capabilities', {})}")
        else:
            print("❌ Initialize failed:", response.get("error"))
            
        # Test 2: List tools
        print("\n2. Testing list_tools...")
        response = client.send_request("tools/list", {})
        
        if "result" in response:
            tools = response["result"].get("tools", [])
            print(f"✅ Found {len(tools)} tools:")
            for tool in tools:
                print(f"   - {tool['name']}: {tool.get('description', 'No description')}")
        else:
            print("❌ List tools failed:", response.get("error"))
            
        # Test 3: Call a tool
        print("\n3. Testing tool calls...")
        
        # Test analyze_text
        test_text = "Hello world!\nThis is a test.\nWith multiple lines."
        response = client.send_request("tools/call", {
            "name": "analyze_text",
            "arguments": {"text": test_text}
        })
        
        if "result" in response:
            print("✅ analyze_text successful!")
            result = response["result"]
            if "content" in result and result["content"]:
                content = result["content"][0].get("text", "")
                print(f"   Result: {content}")
        else:
            print("❌ analyze_text failed:", response.get("error"))
            
        # Test to_uppercase
        response = client.send_request("tools/call", {
            "name": "to_uppercase",
            "arguments": {"text": "hello world"}
        })
        
        if "result" in response:
            print("✅ to_uppercase successful!")
            result = response["result"]
            if "content" in result and result["content"]:
                content = result["content"][0].get("text", "")
                print(f"   Result: {content}")
        else:
            print("❌ to_uppercase failed:", response.get("error"))
            
        # Test summarize with optional parameter
        long_text = " ".join([f"word{i}" for i in range(100)])
        response = client.send_request("tools/call", {
            "name": "summarize",
            "arguments": {
                "text": long_text,
                "max_words": 10
            }
        })
        
        if "result" in response:
            print("✅ summarize successful!")
            result = response["result"]
            if "content" in result and result["content"]:
                content = result["content"][0].get("text", "")
                print(f"   Result: {content}")
        else:
            print("❌ summarize failed:", response.get("error"))
            
        print("\n✅ All tests completed!")
        
    except Exception as e:
        print(f"\n❌ Error: {e}")
        sys.exit(1)
    finally:
        client.close()
        
if __name__ == "__main__":
    main()
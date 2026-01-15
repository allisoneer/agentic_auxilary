#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "mcp",
# ]
# ///
"""
MCP test client using uv to run with the official Python SDK.

Run this script with:
    uv run test_with_uv.py
"""

import asyncio
import os


async def test_mcp_server():
    """Test the MCP server functionality using the official SDK."""
    # Import after uv has set up the environment
    from mcp import ClientSession, StdioServerParameters
    from mcp.client.stdio import stdio_client

    print("🧪 MCP Server Test Client (Using Official SDK via uv)")
    print("=" * 60)

    # Configure the server parameters - run from current directory
    server_params = StdioServerParameters(
        command="cargo",
        args=["run"],
        env=os.environ.copy()
    )

    try:
        # Connect to the server
        print("\n1. Connecting to MCP server...")
        async with stdio_client(server_params) as (read, write):
            async with ClientSession(read, write) as session:
                # Initialize the connection
                print("   Initializing connection...")
                await session.initialize()
                print("✅ Connected and initialized!")

                # Test 2: List tools
                print("\n2. Testing list_tools...")
                tools_result = await session.list_tools()
                tools = tools_result.tools if hasattr(tools_result, 'tools') else []

                print(f"✅ Found {len(tools)} tools:")
                for tool in tools:
                    print(f"   - {tool.name}: {tool.description or 'No description'}")

                # Test 3: Call tools
                print("\n3. Testing tool calls...")

                # Test analyze_text
                test_text = "Hello world!\nThis is a test.\nWith multiple lines."
                try:
                    result = await session.call_tool(
                        "analyze_text",
                        arguments={"text": test_text}
                    )
                    print("✅ analyze_text successful!")
                    if hasattr(result, 'content') and result.content:
                        content = result.content[0]
                        if hasattr(content, 'text'):
                            print(f"   Result: {content.text}")
                        else:
                            print(f"   Result: {content}")
                except Exception as e:
                    print(f"❌ analyze_text failed: {e}")

                # Test to_uppercase
                try:
                    result = await session.call_tool(
                        "to_uppercase",
                        arguments={"text": "hello world"}
                    )
                    print("✅ to_uppercase successful!")
                    if hasattr(result, 'content') and result.content:
                        content = result.content[0]
                        if hasattr(content, 'text'):
                            print(f"   Result: {content.text}")
                        else:
                            print(f"   Result: {content}")
                except Exception as e:
                    print(f"❌ to_uppercase failed: {e}")

                # Test summarize with optional parameter
                long_text = " ".join([f"word{i}" for i in range(100)])
                try:
                    result = await session.call_tool(
                        "summarize",
                        arguments={
                            "text": long_text,
                            "max_words": 10
                        }
                    )
                    print("✅ summarize successful!")
                    if hasattr(result, 'content') and result.content:
                        content = result.content[0]
                        if hasattr(content, 'text'):
                            print(f"   Result: {content.text}")
                        else:
                            print(f"   Result: {content}")
                except Exception as e:
                    print(f"❌ summarize failed: {e}")

                print("\n✅ All tests completed!")

    except Exception as e:
        print(f"\n❌ Error: {e}")
        import traceback
        traceback.print_exc()
        raise


if __name__ == "__main__":
    asyncio.run(test_mcp_server())
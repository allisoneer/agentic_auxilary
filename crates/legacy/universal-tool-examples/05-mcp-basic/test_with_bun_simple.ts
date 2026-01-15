#!/usr/bin/env bun
/**
 * MCP test client using bun to run TypeScript directly with the official SDK.
 *
 * Run this script with:
 *     bun run test_with_bun_simple.ts
 *
 * Bun will auto-install the dependencies on first run!
 *
 * Note: The SDK will be automatically installed from npm.
 */

// Import from the published npm package
// Bun will auto-install this on first run
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";

async function testMcpServer() {
  console.log('🧪 MCP Server Test Client (Using Official SDK from npm)');
  console.log('='.repeat(60));

  try {
    // Create client
    console.log('\n1. Creating MCP client...');
    const client = new Client({
      name: 'test-client',
      version: '1.0.0'
    });

    // Create transport
    console.log('   Creating stdio transport...');
    const transport = new StdioClientTransport({
      command: 'cargo',
      args: ['run'],
      stderr: 'inherit' as const
    });

    // Connect
    console.log('   Connecting to server...');
    await client.connect(transport);
    console.log('✅ Connected and initialized!');

    // Test 2: List tools
    console.log('\n2. Testing list_tools...');
    const toolsResult = await client.listTools();
    const tools = toolsResult.tools || [];

    console.log(`✅ Found ${tools.length} tools:`);
    for (const tool of tools) {
      console.log(`   - ${tool.name}: ${tool.description || 'No description'}`);
    }

    // Test 3: Call tools
    console.log('\n3. Testing tool calls...');

    // Test analyze_text
    const testText = 'Hello world!\nThis is a test.\nWith multiple lines.';
    try {
      const result = await client.callTool({
        name: 'analyze_text',
        arguments: { text: testText }
      });
      console.log('✅ analyze_text successful!');
      if (result.content && result.content[0]) {
        const content = result.content[0];
        if ('text' in content) {
          console.log(`   Result: ${content.text}`);
        } else {
          console.log(`   Result: ${JSON.stringify(content)}`);
        }
      }
    } catch (e: any) {
      console.log(`❌ analyze_text failed: ${e.message}`);
    }

    // Test to_uppercase
    try {
      const result = await client.callTool({
        name: 'to_uppercase',
        arguments: { text: 'hello world' }
      });
      console.log('✅ to_uppercase successful!');
      if (result.content && result.content[0]) {
        const content = result.content[0];
        if ('text' in content) {
          console.log(`   Result: ${content.text}`);
        } else {
          console.log(`   Result: ${JSON.stringify(content)}`);
        }
      }
    } catch (e: any) {
      console.log(`❌ to_uppercase failed: ${e.message}`);
    }

    // Test summarize with optional parameter
    const longText = Array.from({length: 100}, (_, i) => `word${i}`).join(' ');
    try {
      const result = await client.callTool({
        name: 'summarize',
        arguments: {
          text: longText,
          max_words: 10
        }
      });
      console.log('✅ summarize successful!');
      if (result.content && result.content[0]) {
        const content = result.content[0];
        if ('text' in content) {
          console.log(`   Result: ${content.text}`);
        } else {
          console.log(`   Result: ${JSON.stringify(content)}`);
        }
      }
    } catch (e: any) {
      console.log(`❌ summarize failed: ${e.message}`);
    }

    console.log('\n✅ All tests completed!');

    // Close connection
    await client.close();
    process.exit(0);
  } catch (error: any) {
    console.error(`\n❌ Error: ${error.message}`);
    console.error(error.stack);
    process.exit(1);
  }
}

// Run the test
testMcpServer().catch(error => {
  console.error('Failed to run test:', error);
  process.exit(1);
});
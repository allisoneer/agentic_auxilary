#!/usr/bin/env node
// Simple Node.js MCP test client

const { spawn } = require('child_process');
const readline = require('readline');

// Start the MCP server
const server = spawn('cargo', ['run', '-p', 'example-05-mcp-basic'], {
  stdio: ['pipe', 'pipe', 'pipe']
});

// Create readline interface for server stdout
const rl = readline.createInterface({
  input: server.stdout,
  crlfDelay: Infinity
});

// Track request ID
let requestId = 0;
let initializeRequestId = null;

// Handle server output
rl.on('line', (line) => {
  if (line.startsWith('{')) {
    try {
      const response = JSON.parse(line);
      console.log('Response:', JSON.stringify(response, null, 2));

      // When initialize result arrives, send initialized notification
      if (response.id === initializeRequestId && response.result) {
        console.log('\nHandshake: Initialize response received, sending notifications/initialized');
        sendNotification('notifications/initialized');

        // Now safe to proceed with other requests
        setTimeout(() => sendRequest('tools/list'), 200);
        setTimeout(() => sendRequest('tools/call', {
          name: 'analyze_text',
          arguments: { text: 'Hello world!\nThis is a test.' }
        }), 500);
      }
    } catch (e) {
      console.log('Non-JSON output:', line);
    }
  }
});

// Handle server errors
server.stderr.on('data', (data) => {
  const str = data.toString();
  if (!str.includes('warning:') && !str.includes('Finished') && !str.includes('Compiling')) {
    console.error('Server stderr:', str);
  }
});

// Send a request
function sendRequest(method, params = {}) {
  requestId++;
  const request = {
    jsonrpc: '2.0',
    method: method,
    params: params,
    id: requestId
  };

  console.log('\nSending:', JSON.stringify(request));
  server.stdin.write(JSON.stringify(request) + '\n');
}

// Send a notification
function sendNotification(method, params = {}) {
  const notification = {
    jsonrpc: '2.0',
    method: method,
    params: params
  };
  console.log('\nSending notification:', JSON.stringify(notification));
  server.stdin.write(JSON.stringify(notification) + '\n');
}

// Wait for server to start
setTimeout(() => {
  console.log('Testing MCP server...\n');

  // Test 1: Initialize
  initializeRequestId = requestId + 1;
  sendRequest('initialize', {
    protocolVersion: '1.0',
    clientCapabilities: {}
  });

  // The rest of the flow will be triggered by the response handler
  // after receiving the initialize response and sending notifications/initialized

  // Close after tests
  setTimeout(() => {
    console.log('\nTests complete, closing server...');
    server.kill();
    process.exit(0);
  }, 3000);

}, 2000);

// Handle process exit
process.on('exit', () => {
  server.kill();
});
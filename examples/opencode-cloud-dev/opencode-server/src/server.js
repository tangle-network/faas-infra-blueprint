import express from 'express';
import cors from 'cors';
import { createOpencodeClient } from '@opencode-ai/sdk';

const app = express();
const EXPRESS_PORT = 4096;
const OPENCODE_PORT = 5173;

app.use(cors());
app.use(express.json());

let opencodeClient;

// Initialize OpenCode client to connect to the CLI server
async function initializeClient() {
  try {
    console.log('📡 Connecting to OpenCode server on port 5173...');
    opencodeClient = await createOpencodeClient({
      baseUrl: `http://127.0.0.1:${OPENCODE_PORT}`,
      responseStyle: 'data',
    });
    console.log('✅ OpenCode client connected to server');
    return true;
  } catch (error) {
    console.error('Failed to connect to OpenCode server:', error);
    // Retry connection
    setTimeout(initializeClient, 2000);
    return false;
  }
}

// Main chat endpoint that proxies to OpenCode
app.post('/api/prompt', async (req, res) => {
  const { prompt } = req.body;

  if (!opencodeClient) {
    return res.status(503).json({ error: 'OpenCode not initialized' });
  }

  // Set up SSE headers
  res.writeHead(200, {
    'Content-Type': 'text/event-stream',
    'Cache-Control': 'no-cache',
    'Connection': 'keep-alive',
  });

  try {
    // Create a session with OpenCode
    const session = await opencodeClient.session.create({
      body: { title: 'Prompt Session' }
    });

    // Send the prompt to OpenCode
    const response = await opencodeClient.session.prompt({
      path: { id: session.id },
      body: {
        parts: [{ type: 'text', text: prompt }]
      }
    });

    // Stream the response back to the client
    if (response && response.text) {
      // Send the response in chunks for streaming effect
      const chunks = response.text.match(/.{1,50}/g) || [];
      for (const chunk of chunks) {
        res.write(`data: ${JSON.stringify({ chunk })}\n\n`);
        await new Promise(r => setTimeout(r, 20));
      }
    }

    res.write(`data: ${JSON.stringify({ done: true })}\n\n`);
  } catch (error) {
    console.error('Error processing prompt:', error);
    res.write(`data: ${JSON.stringify({ error: error.message })}\n\n`);
  }

  res.end();
});

// Alternative streaming endpoint
app.post('/api/chat', async (req, res) => {
  const { prompt } = req.body;

  if (!opencodeClient) {
    return res.status(503).json({ error: 'OpenCode not initialized' });
  }

  // Set up SSE headers
  res.writeHead(200, {
    'Content-Type': 'text/event-stream',
    'Cache-Control': 'no-cache',
    'Connection': 'keep-alive',
  });

  try {
    // Try different API methods based on what's available
    let response;

    // Try session-based approach first
    try {
      const session = await opencodeClient.session.create({
        body: { title: 'Chat Session' }
      });

      response = await opencodeClient.session.prompt({
        path: { id: session.id },
        body: {
          parts: [{ type: 'text', text: prompt }]
        }
      });
    } catch (e) {
      console.log('Session API not available, trying direct completion...');
      // Fallback to direct completion if available
      if (opencodeClient.complete) {
        response = await opencodeClient.complete({
          prompt: prompt,
          stream: true
        });
      } else {
        throw new Error('No suitable API method available');
      }
    }

    // Handle the response - extract text from OpenCode response format
    if (response) {
      let textContent = '';

      // OpenCode response format: response.parts[] (at top level)
      if (response.parts && Array.isArray(response.parts)) {
        // Extract all text parts
        for (const part of response.parts) {
          if (part.type === 'text' && part.text) {
            textContent += part.text;
          }
        }
      } else if (response.info && response.info.parts && Array.isArray(response.info.parts)) {
        // Fallback: check if parts are in info object
        for (const part of response.info.parts) {
          if (part.type === 'text' && part.text) {
            textContent += part.text;
          }
        }
      } else if (response.text) {
        // Fallback if response has direct text property
        textContent = response.text;
      } else if (typeof response === 'string') {
        textContent = response;
      }

      // Send the text content as chunks for streaming effect
      if (textContent) {
        const chunks = textContent.match(/.{1,100}/g) || [textContent];
        for (const chunk of chunks) {
          res.write(`data: ${JSON.stringify({ chunk })}\n\n`);
          await new Promise(r => setTimeout(r, 30));
        }
      } else {
        // No text found
        console.log('Warning: No text content extracted from response');
        res.write(`data: ${JSON.stringify({ chunk: 'No response generated' })}\n\n`);
      }
    }

    res.write(`data: ${JSON.stringify({ done: true })}\n\n`);
  } catch (error) {
    console.error('Error in chat endpoint:', error);
    res.write(`data: ${JSON.stringify({ error: error.message })}\n\n`);
  }

  res.end();
});

// Health check endpoint
app.get('/health', async (req, res) => {
  const clientReady = !!opencodeClient;

  res.json({
    status: clientReady ? 'healthy' : 'initializing',
    express: 'running',
    opencodeClient: clientReady ? 'connected' : 'connecting',
    ready: clientReady
  });
});

// Graceful shutdown
process.on('SIGTERM', async () => {
  console.log('Shutting down gracefully...');
  process.exit(0);
});

// Start Express server and initialize client
app.listen(EXPRESS_PORT, '0.0.0.0', async () => {
  console.log(`🚀 Express Proxy Server on port ${EXPRESS_PORT}`);
  console.log(`📡 Will connect to OpenCode server at localhost:${OPENCODE_PORT}`);

  // Give OpenCode server time to start
  setTimeout(async () => {
    const initialized = await initializeClient();
    if (!initialized) {
      console.log('OpenCode server not ready yet, will retry...');
    }
  }, 3000);
});
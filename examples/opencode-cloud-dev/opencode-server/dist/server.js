"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const express_1 = __importDefault(require("express"));
const cors_1 = __importDefault(require("cors"));
const promises_1 = __importDefault(require("fs/promises"));
const path_1 = __importDefault(require("path"));
const openai_1 = __importDefault(require("openai"));
const sdk_1 = __importDefault(require("@anthropic-ai/sdk"));
const app = (0, express_1.default)();
const PORT = Number(process.env.PORT) || 4096;
app.use((0, cors_1.default)());
app.use(express_1.default.json({ limit: '50mb' }));
app.use(express_1.default.text({ limit: '50mb' }));
// AI Provider configuration
const AI_PROVIDER = process.env.AI_PROVIDER || 'openai'; // 'openai' or 'anthropic'
const API_KEY = process.env.AI_API_KEY || '';
// Initialize AI client based on provider
let aiClient = null;
if (AI_PROVIDER === 'anthropic' && API_KEY) {
    aiClient = new sdk_1.default({
        apiKey: API_KEY
    });
}
else if (AI_PROVIDER === 'openai' && API_KEY) {
    aiClient = new openai_1.default({
        apiKey: API_KEY
    });
}
// Generic chat endpoint - forwards ANY prompt to AI
app.post('/api/chat', async (req, res) => {
    try {
        const prompt = typeof req.body === 'string' ? req.body : req.body.prompt || req.body.text;
        if (!prompt) {
            return res.status(400).json({ error: 'No prompt provided' });
        }
        console.log('📝 Received prompt:', prompt.substring(0, 100) + '...');
        // Set up streaming response
        res.writeHead(200, {
            'Content-Type': 'text/event-stream',
            'Cache-Control': 'no-cache',
            'Connection': 'keep-alive',
            'Transfer-Encoding': 'chunked'
        });
        if (!aiClient) {
            // If no AI configured, return error
            const error = 'No AI provider configured. Set AI_PROVIDER and AI_API_KEY environment variables.';
            res.write(`data: ${JSON.stringify({ chunk: error })}\n\n`);
            res.write(`data: ${JSON.stringify({ done: true })}\n\n`);
            res.end();
            return;
        }
        // Forward to AI and stream response
        if (AI_PROVIDER === 'anthropic') {
            const client = aiClient;
            const stream = await client.messages.create({
                model: 'claude-3-opus-20240229',
                max_tokens: 4096,
                messages: [{ role: 'user', content: prompt }],
                stream: true,
            });
            for await (const chunk of stream) {
                if (chunk.type === 'content_block_delta' && chunk.delta.type === 'text_delta') {
                    res.write(`data: ${JSON.stringify({ chunk: chunk.delta.text })}\n\n`);
                }
            }
        }
        else if (AI_PROVIDER === 'openai') {
            const client = aiClient;
            const stream = await client.chat.completions.create({
                model: 'gpt-4',
                messages: [{ role: 'user', content: prompt }],
                stream: true,
            });
            for await (const chunk of stream) {
                const content = chunk.choices[0]?.delta?.content;
                if (content) {
                    res.write(`data: ${JSON.stringify({ chunk: content })}\n\n`);
                }
            }
        }
        // Send completion signal
        res.write(`data: ${JSON.stringify({ done: true })}\n\n`);
        res.end();
    }
    catch (error) {
        console.error('❌ Error processing prompt:', error);
        res.status(500).json({
            error: 'Failed to process prompt',
            details: error instanceof Error ? error.message : String(error)
        });
    }
});
// Execute code endpoint - runs generated code locally
app.post('/api/execute', async (req, res) => {
    try {
        const { language, code, filename } = req.body;
        if (!code) {
            return res.status(400).json({ error: 'No code provided' });
        }
        // Save code to temp directory
        const tempDir = `/tmp/opencode-${Date.now()}`;
        await promises_1.default.mkdir(tempDir, { recursive: true });
        const filePath = path_1.default.join(tempDir, filename || 'main.rs');
        await promises_1.default.writeFile(filePath, code);
        console.log(`💾 Saved code to ${filePath}`);
        res.json({
            success: true,
            path: filePath,
            message: 'Code saved successfully'
        });
    }
    catch (error) {
        console.error('❌ Error executing code:', error);
        res.status(500).json({
            error: 'Failed to execute code',
            details: error instanceof Error ? error.message : String(error)
        });
    }
});
// Health check endpoint
app.get('/health', (req, res) => {
    res.json({
        status: 'healthy',
        service: 'opencode-server',
        ai_provider: AI_PROVIDER,
        ai_configured: !!aiClient
    });
});
// Root endpoint
app.get('/', (req, res) => {
    res.json({
        message: 'OpenCode AI Gateway',
        version: '3.0.0',
        endpoints: {
            chat: 'POST /api/chat - Send any prompt to AI',
            execute: 'POST /api/execute - Save generated code',
            health: 'GET /health - Service health check'
        },
        configuration: {
            ai_provider: AI_PROVIDER,
            ai_configured: !!aiClient
        }
    });
});
app.listen(PORT, '0.0.0.0', () => {
    console.log(`🚀 OpenCode AI Gateway listening on port ${PORT}`);
    console.log(`🤖 AI Provider: ${AI_PROVIDER}`);
    console.log(`✅ AI Configured: ${!!aiClient}`);
    console.log(`📝 Send any prompt to: http://localhost:${PORT}/api/chat`);
});

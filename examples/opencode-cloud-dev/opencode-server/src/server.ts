import { createOpencodeServer } from "@opencode-ai/sdk";

const PORT = Number(process.env.PORT) || 4096;
const MODEL = process.env.AI_MODEL || "grok-code-fast-1";
const API_KEY = process.env.AI_API_KEY || "";

async function main() {
    // Create the OpenCode server with configurable model
    const server = await createOpencodeServer({
        hostname: "0.0.0.0",
        port: PORT,
        config: {
            model: MODEL,
            apiKey: API_KEY
        }
    });

    console.log(`🚀 OpenCode Server running at ${server.url}`);
    console.log(`🤖 Model: ${MODEL}`);
    console.log(`✅ Ready to accept ANY prompt at ${server.url}/api/chat`);

    // Handle graceful shutdown
    process.on('SIGINT', () => {
        console.log('\n📴 Shutting down OpenCode server...');
        server.close();
        process.exit(0);
    });

    process.on('SIGTERM', () => {
        console.log('\n📴 Shutting down OpenCode server...');
        server.close();
        process.exit(0);
    });
}

main().catch(console.error);
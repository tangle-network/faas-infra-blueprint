#!/bin/sh

# Ensure opencode binary is accessible
export PATH="/usr/local/bin:$PATH"

# Start the OpenCode CLI server in the background
echo "Starting OpenCode server via CLI..."
/usr/local/bin/opencode serve --hostname=127.0.0.1 --port=5173 &
OPENCODE_PID=$!

# Wait for OpenCode to start
echo "Waiting for OpenCode server to start..."
sleep 5

# Check if OpenCode is running
if ! kill -0 $OPENCODE_PID 2>/dev/null; then
    echo "OpenCode server failed to start"
    exit 1
fi

echo "OpenCode server running on port 5173 (PID: $OPENCODE_PID)"

# Start the Express server
echo "Starting Express proxy server..."
exec node src/server.js
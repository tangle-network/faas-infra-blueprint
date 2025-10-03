FROM node:20-slim

WORKDIR /opencode-server

# Install OpenCode CLI which the SDK needs internally
# The SDK's createOpencodeServer spawns the CLI binary
RUN npm install -g opencode-ai && \
    # Create symlink in multiple locations to ensure it's found
    ln -sf /usr/local/bin/opencode /usr/bin/opencode && \
    ln -sf /usr/local/bin/opencode /bin/opencode && \
    # Make sure it's executable
    chmod +x /usr/local/bin/opencode

# Copy package files and install dependencies
COPY package.json .
RUN npm install

# Copy application files
COPY src/ src/
COPY start-server.sh .
RUN chmod +x start-server.sh

# Set PATH explicitly for child processes
ENV PATH="/usr/local/bin:/usr/bin:/bin:${PATH}"
ENV NODE_PATH="/usr/local/lib/node_modules"

# Expose Express proxy port (OpenCode will run on internal port 5173)
EXPOSE 4096

# Use the wrapper script that starts both servers
CMD ["./start-server.sh"]
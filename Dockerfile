# Multi-stage Dockerfile for optimized API Gateway build
# Stage 1: Build the application
FROM rust:1.75-slim as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Create a new directory for the app
WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create a dummy main.rs to build dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    echo "pub fn dummy() {}" > src/lib.rs

# Build dependencies (this layer will be cached)
RUN cargo build --release && \
    rm -rf src

# Copy the actual source code
COPY src ./src

# Build the application
# Touch main.rs to ensure it rebuilds
RUN touch src/main.rs && \
    cargo build --release --bin gateway

# Stage 2: Create the runtime image
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN useradd -m -u 1000 gateway && \
    mkdir -p /app/config && \
    chown -R gateway:gateway /app

# Switch to non-root user
USER gateway
WORKDIR /app

# Copy the binary from builder
COPY --from=builder /app/target/release/gateway /app/gateway

# Copy default configuration
COPY --chown=gateway:gateway config/ /app/config/

# Expose ports
EXPOSE 8080
EXPOSE 8443

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["/app/gateway", "--health-check"]

# Set environment variables
ENV RUST_LOG=info
ENV GATEWAY_CONFIG=/app/config/gateway.yaml

# Run the binary
ENTRYPOINT ["/app/gateway"]
CMD []

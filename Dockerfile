# Build Stage for UI
FROM node:22-slim AS ui-builder
WORKDIR /app/ui
COPY ui/package*.json ./
RUN rm -f package-lock.json && npm install
COPY ui/ ./
RUN npm run build

# Build Stage for Rust Binary
FROM rust:latest AS builder
WORKDIR /app

# Install build dependencies covering SQLite/Rusqlite needs
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    gcc \
    g++ \
    make \
    perl \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests to cache dependencies
COPY Cargo.toml Cargo.lock ./
# Create dummy src to build dependencies first
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# Copy actual source code
COPY src ./src
# Copy built UI assets from ui-builder
COPY --from=ui-builder /app/ui/dist ./ui/dist

# Build the actual application
# Touch main.rs to ensure rebuild
RUN touch src/main.rs
RUN cargo build --release

# Final Runtime Stage
FROM debian:bookworm-slim
WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/stackhouse /app/stackhouse

# Create directory for database
RUN mkdir -p /data
ENV STACKHOUSE_PATH=/data/stackhouse.db
ENV STACKHOUSE_PORT=8080
ENV STACKHOUSE_HOST=0.0.0.0
ENV QDRANT_URL=http://localhost:6333

# Expose port
EXPOSE 8080

# Run the binary
CMD ["/app/stackhouse", "serve"]

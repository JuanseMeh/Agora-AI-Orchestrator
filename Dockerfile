# Fixed Multi-stage Dockerfile for Rust AI Orchestration Service (gRPC + proto codegen)
# Stage 1: Build
# ============================================
# Using rust:1.88 for Rust 2024 edition and dependency support
FROM --platform=$TARGETPLATFORM rust:1.88-alpine AS builder

# Install build dependencies (Alpine uses apk)
RUN apk add --no-cache \
    pkgconfig \
    openssl-dev \
    musl-dev \
    gcc \
    protobuf-dev \
    protoc

# Create app directory
WORKDIR /app

# Copy dependency files first for better caching
COPY Cargo.toml Cargo.lock ./
COPY build.rs ./
COPY proto ./proto/
COPY src ./src

# Build the application
# Using release profile for optimized binary
RUN cargo build --release --bin ai

# ============================================
# Stage 2: Production
# ============================================
FROM --platform=$TARGETPLATFORM alpine:latest AS production

# Install runtime dependencies
RUN apk add --no-cache \
    libssl3 \
    ca-certificates \
    curl \
    && adduser -D -s /bin/sh appuser

# Create app directory
WORKDIR /app

# Copy the built binary from builder
COPY --from=builder /app/target/release/ai /usr/local/bin/ai

# Switch to non-root user
USER appuser

# Run the application
CMD ["ai"]

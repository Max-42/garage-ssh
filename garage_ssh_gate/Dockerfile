ARG BUILD_FROM
FROM rust:1.85-alpine AS builder

# Install build dependencies
RUN apk add --no-cache \
    musl-dev \
    pkgconfig \
    openssl-dev \
    openssl-libs-static \
    perl \
    make

# Create app directory
WORKDIR /build

# Copy manifest first for better layer caching
COPY src/Cargo.toml ./Cargo.toml

# Create a dummy main.rs to pre-build dependencies
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release 2>/dev/null || true && \
    rm -rf src

# Copy actual source code
COPY src/src/ ./src/
COPY src/web/ ./web/

# Build release binary
RUN cargo build --release

# --- Runtime stage ---
FROM $BUILD_FROM

# Install runtime dependencies (minimal)
RUN apk add --no-cache \
    ca-certificates \
    bash \
    jq \
    libgcc

# Copy the built binary
COPY --from=builder /build/target/release/garage-ssh-gate /usr/bin/garage-ssh-gate
RUN chmod a+x /usr/bin/garage-ssh-gate

# Copy run script
COPY run.sh /
RUN chmod a+x /run.sh

# Create data directory
RUN mkdir -p /data

# Expose ports
EXPOSE 2242
EXPOSE 8099

LABEL \
    io.hass.version="VERSION" \
    io.hass.type="addon" \
    io.hass.arch="aarch64|amd64|armv7|i386"

CMD [ "/run.sh" ]

ARG BUILDER_FROM=docker.io/rust:1.85-alpine
ARG BUILD_FROM=ghcr.io/home-assistant/amd64-base:latest
ARG VERSION=dev

FROM --platform=$TARGETPLATFORM ${BUILDER_FROM} AS builder

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

# Copy manifests first for better layer caching
COPY src/Cargo.toml ./Cargo.toml
COPY src/Cargo.lock ./Cargo.lock

# Copy actual source code
COPY src/src/ ./src/
COPY src/web/ ./web/

# Build release binary
RUN cargo build --release --locked && \
    mkdir -p /build/out && \
    cp "/build/target/release/garage-ssh-gate" /build/out/garage-ssh-gate

# --- Runtime stage ---
ARG BUILD_FROM
FROM ${BUILD_FROM}
ARG VERSION=dev

# Install runtime dependencies (minimal)
RUN apk add --no-cache \
    ca-certificates \
    bash \
    jq \
    libgcc

# Copy the built binary
COPY --from=builder /build/out/garage-ssh-gate /usr/bin/garage-ssh-gate
RUN chmod a+x /usr/bin/garage-ssh-gate

# Copy run script
COPY run.sh /
RUN chmod a+x /run.sh

# Create data directory
RUN mkdir -p /data

# Expose ports
EXPOSE 2242
EXPOSE 8099

ENV APP_VERSION="${VERSION}"

LABEL \
    io.hass.version="${VERSION}" \
    io.hass.type="addon" \
    io.hass.arch="aarch64|amd64"

CMD [ "/run.sh" ]

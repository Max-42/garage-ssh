ARG BUILDER_FROM=docker.io/rust:1.85-alpine
ARG BUILD_FROM=ghcr.io/home-assistant/amd64-base:latest
ARG VERSION=dev

FROM --platform=$BUILDPLATFORM ${BUILDER_FROM} AS builder
ARG TARGETARCH

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

# Select Rust target for the requested Docker architecture
RUN case "${TARGETARCH}" in \
            amd64) echo "x86_64-unknown-linux-musl" > /tmp/rust_target ;; \
            arm64|aarch64) echo "aarch64-unknown-linux-musl" > /tmp/rust_target ;; \
            *) echo "Unsupported TARGETARCH: ${TARGETARCH}" && exit 1 ;; \
        esac

RUN rustup target add "$(cat /tmp/rust_target)"

# Copy manifests first for better layer caching
COPY src/Cargo.toml ./Cargo.toml
COPY src/Cargo.lock ./Cargo.lock

# Copy actual source code
COPY src/src/ ./src/
COPY src/web/ ./web/

# Build release binary
RUN RUST_TARGET="$(cat /tmp/rust_target)" && \
    cargo build --release --locked --target "${RUST_TARGET}" && \
    mkdir -p /build/out && \
    cp "/build/target/${RUST_TARGET}/release/garage-ssh-gate" /build/out/garage-ssh-gate

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

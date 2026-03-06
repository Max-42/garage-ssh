#!/usr/bin/with-contenv bashio

bashio::log.info "Starting Garage SSH Gate..."

# Read configuration
SSH_PORT=$(bashio::config 'ssh_port')
LOG_LEVEL=$(bashio::config 'log_level')

bashio::log.info "SSH Port: ${SSH_PORT}"
bashio::log.info "Log Level: ${LOG_LEVEL}"

# Ensure data directory exists
mkdir -p /data

# Run the Rust binary
exec /usr/bin/garage-ssh-gate

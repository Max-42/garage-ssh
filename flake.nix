{
  description = "Garage SSH Gate - Development Environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        rustToolchain = pkgs.rust-bin.stable."1.85.0".default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
        };

        # Reusable build script for the container
        buildScript = pkgs.writeShellScriptBin "build-container" ''
          set -euo pipefail
          # Always work from repo root regardless of cwd
          REPO_ROOT=$(${pkgs.git}/bin/git rev-parse --show-toplevel)
          cd "$REPO_ROOT"
          BASE="ghcr.io/home-assistant/amd64-base:latest"
          TAG="''${1:-garage-ssh-gate:local}"
          echo "🐳 Building container image: $TAG"
          echo "   Base:      $BASE"
          echo "   Repo root: $REPO_ROOT"
          ${pkgs.podman}/bin/podman build \
            --build-arg BUILD_FROM="$BASE" \
            --tag "$TAG" \
            --file "$REPO_ROOT/garage_ssh_gate/Dockerfile" \
            "$REPO_ROOT/garage_ssh_gate"
          echo "✅ Build complete: $TAG"
        '';

        testScript = pkgs.writeShellScriptBin "test-container" ''
          set -euo pipefail
          TAG="''${1:-garage-ssh-gate:local}"
          echo "🧪 Testing container: $TAG"

          # 1. Image exists?
          ${pkgs.podman}/bin/podman image inspect "$TAG" > /dev/null 2>&1 \
            || { echo "❌ Image $TAG not found - run build-container first"; exit 1; }
          echo "  ✓ Image exists"

          # 2. Binary present and executable
          ${pkgs.podman}/bin/podman run --rm --entrypoint /bin/sh "$TAG" \
            -c "test -x /usr/bin/garage-ssh-gate && echo ok" \
            && echo "  ✓ Binary present and executable"

          # 3. Smoke-test: starts up and stays running for 3s
          echo "  → Starting container for smoke test (3s)..."
          CID=$(${pkgs.podman}/bin/podman run -d \
            -p 12242:2242 -p 18099:8099 \
            "$TAG" /bin/sh -c "
              mkdir -p /data
              printf '{\"ssh_port\":2242,\"webhook_url\":\"\",\"home_latitude\":0,\"home_longitude\":0,\"geofence_radius_km\":15,\"geofence_override_timeout_sec\":45,\"tofu_timeout_sec\":45,\"untrusted_key_retention_days\":21,\"expected_json_version\":\"1.0.1\",\"log_level\":\"info\",\"host_key_pem\":\"\"}' \
                > /data/options.json
              exec /usr/bin/garage-ssh-gate
            ")
          sleep 3
          STATUS=$(${pkgs.podman}/bin/podman inspect "$CID" --format '{{.State.Status}}' 2>/dev/null || echo "gone")
          if [ "$STATUS" = "running" ]; then
            echo "  ✓ Container stayed up for 3s"
            ${pkgs.podman}/bin/podman stop "$CID" > /dev/null 2>&1 || true
          else
            echo "  ❌ Container exited early (status: $STATUS)"
            ${pkgs.podman}/bin/podman logs "$CID" 2>/dev/null || true
            ${pkgs.podman}/bin/podman rm "$CID" > /dev/null 2>&1 || true
            exit 1
          fi

          echo "✅ All tests passed for $TAG"
        '';
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            pkg-config
            openssl
            openssl.dev
            libiconv
            git
            docker
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.darwin.apple_sdk.frameworks.Security
            pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
          ];

          shellHook = ''
            echo "🦀 Garage SSH Gate - Dev Environment"
            echo "   Rust: $(rustc --version)"
            echo "   Cargo: $(cargo --version)"
            echo "   Docker: $(docker --version)"
            echo ""
            echo "Commands:"
            echo "  cargo build                      - Build debug"
            echo "  cargo build --release            - Build release"
            echo "  cargo build --profile release-local  - Fast release (all cores)"
            echo "  cargo test                       - Run tests"
            echo "  cargo clippy                     - Lint"
            echo "  cargo fmt                        - Format"
            echo ""
          '';

          OPENSSL_DIR = "${pkgs.openssl.dev}";
          OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
          PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
        };

        # Container dev shell: podman + buildah, no daemon needed (rootless)
        devShells.docker = pkgs.mkShell {
          buildInputs = with pkgs; [
            podman
            buildah
            skopeo
            buildScript
            testScript
          ];

          shellHook = ''
            echo "🐳 Garage SSH Gate - Container Environment"
            echo "   Podman: $(podman --version)"
            echo "   Buildah: $(buildah --version | head -1)"
            echo ""
            echo "Commands:"
            echo "  build-container [tag]   - Build the HA add-on container image"
            echo "  test-container  [tag]   - Smoke-test the built image"
            echo ""
            echo "Example:"
            echo "  build-container garage-ssh-gate:local"
            echo "  test-container  garage-ssh-gate:local"
            echo ""
            # Podman rootless: ensure policy.json and registries.conf exist
            mkdir -p "$HOME/.config/containers"
            if [ ! -f "$HOME/.config/containers/policy.json" ]; then
              echo '{"default":[{"type":"insecureAcceptAnything"}]}' \
                > "$HOME/.config/containers/policy.json"
            fi
            if [ ! -f "$HOME/.config/containers/registries.conf" ]; then
              printf '[registries.search]\nregistries = ["docker.io", "ghcr.io"]\n' \
                > "$HOME/.config/containers/registries.conf"
            fi
            podman system migrate > /dev/null 2>&1 || true
          '';
        };
      }
    );
}

#!/usr/bin/env sh
# rusty-sched installer — Linux / macOS.
# Detects OS + arch, downloads the matching release tarball from GitHub,
# extracts the binary to $PREFIX/bin (default /usr/local/bin), and prints
# next-step hints.
#
# Usage:
#   curl -fsSL https://github.com/jdp5949/rusty-sched/releases/latest/download/install.sh | sh
#   curl -fsSL .../install.sh | PREFIX=$HOME/.local sh
#   curl -fsSL .../install.sh | VERSION=v0.1.0 sh

set -eu

REPO="jdp5949/rusty-sched"
PREFIX="${PREFIX:-/usr/local}"
VERSION="${VERSION:-}"

err() { printf '\033[31merror:\033[0m %s\n' "$*" >&2; exit 1; }
info() { printf '\033[36m==>\033[0m %s\n' "$*"; }

detect_target() {
    os="$(uname -s)"
    arch="$(uname -m)"
    case "$os" in
        Linux)
            case "$arch" in
                x86_64|amd64)  echo "x86_64-unknown-linux-gnu" ;;
                aarch64|arm64) echo "aarch64-unknown-linux-gnu" ;;
                *) err "unsupported Linux arch: $arch" ;;
            esac
            ;;
        Darwin)
            case "$arch" in
                x86_64) echo "x86_64-apple-darwin" ;;
                arm64)  echo "aarch64-apple-darwin" ;;
                *) err "unsupported macOS arch: $arch" ;;
            esac
            ;;
        *) err "unsupported OS: $os (use install.ps1 on Windows)" ;;
    esac
}

resolve_version() {
    if [ -n "$VERSION" ]; then
        echo "$VERSION"
        return
    fi
    info "resolving latest release..."
    tag="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
        | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1)"
    [ -n "$tag" ] || err "could not resolve latest release tag"
    echo "$tag"
}

require_cmd() {
    command -v "$1" >/dev/null 2>&1 || err "missing required command: $1"
}

main() {
    require_cmd curl
    require_cmd tar

    target="$(detect_target)"
    version="$(resolve_version)"

    name="rusty-sched-${version}-${target}"
    url="https://github.com/$REPO/releases/download/${version}/${name}.tar.gz"

    info "downloading $url"
    tmp="$(mktemp -d 2>/dev/null || mktemp -d -t rusty-sched)"
    trap 'rm -rf "$tmp"' EXIT
    curl -fsSL "$url" -o "$tmp/rs.tar.gz"

    info "extracting"
    tar -C "$tmp" -xzf "$tmp/rs.tar.gz"
    src="$tmp/$name/rusty-sched"
    [ -f "$src" ] || err "binary not found in archive: expected $src"

    dest="$PREFIX/bin"
    if [ ! -d "$dest" ]; then
        info "creating $dest"
        mkdir -p "$dest" 2>/dev/null || sudo mkdir -p "$dest"
    fi

    final="$dest/rusty-sched"
    if [ -w "$dest" ]; then
        cp "$src" "$final"
        chmod 0755 "$final"
    else
        info "elevating to write $final"
        sudo cp "$src" "$final"
        sudo chmod 0755 "$final"
    fi

    info "installed: $final"
    "$final" version

    cat <<EOF

Next steps:
  rusty-sched server         # boot the scheduler on :8080
  open http://localhost:8080 # web UI

Docs: https://jdp5949.github.io/rusty-sched/
EOF
}

main "$@"

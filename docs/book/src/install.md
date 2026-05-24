# Install

## Pre-built binary (recommended)

Every release tag (`v0.X.Y`) publishes binaries for:

- Linux x86_64 (gnu + musl)
- macOS x86_64 + aarch64
- Windows x86_64

Grab one from the [Releases page](https://github.com/jdp5949/rusty-sched/releases)
or use the one-liners:

### Linux / macOS

```bash
curl -fsSL https://github.com/jdp5949/rusty-sched/releases/latest/download/install.sh | sh
```

### Windows (PowerShell)

```powershell
irm https://github.com/jdp5949/rusty-sched/releases/latest/download/install.ps1 | iex
```

The installer drops `rusty-sched` into `~/.local/bin` (unix) or
`%LOCALAPPDATA%\Programs\rusty-sched\` (windows). Add it to your PATH if it
isn't already.

## From source

Requires Rust 1.79+:

```bash
git clone https://github.com/jdp5949/rusty-sched
cd rusty-sched
cargo install --path crates/rsched-bin
```

## Docker

```bash
docker run -d --name rusty-sched \
  -p 8080:8080 \
  -e RSCHED_ADMIN_PASSWORD=change-me \
  -v rusty-sched-data:/data \
  ghcr.io/jdp5949/rusty-sched:latest
```

Data dir defaults to `/data` inside the container — mount a volume to
preserve the SQLite DB across restarts.

## Service installation

Once you have the binary on PATH:

```bash
sudo rusty-sched service install   # systemd / launchd / Windows service
sudo systemctl start rusty-sched-server  # linux
```

Service units live under `/etc/systemd/system/rusty-sched-server.service`
(linux), `/Library/LaunchDaemons/io.rustysched.server.plist` (macOS), and
register via `windows-service` on Windows.

## Verify

```bash
rusty-sched version
rusty-sched cli --url http://localhost:8080 list
```

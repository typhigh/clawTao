# ClawTao

Desktop AI Agent powered by a Rust backend with an Electron UI.

## Quick Start

```bash
# Install dependencies
pnpm install

# Build Rust core (first time)
pnpm run core:build

# Start dev
pnpm dev
```

First launch opens Settings automatically. Fill in your LLM provider config and click **Test Connection** to verify, then **Save**.

## Commands

```bash
pnpm dev              # Start dev mode (hot reload)
pnpm run core:build   # Build Rust release binary
pnpm build            # Full build (Rust + Vite + electron-builder)
pnpm test:all         # Run all tests (frontend + Rust)
pnpm test:ui          # Frontend tests only
pnpm test:core        # Rust tests only
```

## Configuration

Config stored at `~/Library/Application Support/clawtao/config.json`:

```json
{
  "provider": "openai",
  "base_url": "https://api.openai.com/v1",
  "model": "gpt-4o",
  "log_level": "info",
  "bash_blocked_commands": ["rm -rf /", "sudo rm", "mkfs."]
}
```

API key is encrypted via Electron `safeStorage` and stored separately in `secrets.json`.

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `SESSION_STORE` | `sqlite` | Session storage backend: `sqlite` or `json` |
| `RUST_LOG` | config value | Override log level (trace/debug/info/warn/error) |

## Built-in Tools

| Tool | Description |
|------|-------------|
| `Read` | Read file contents |
| `Write` | Write content to file |
| `Edit` | Exact string replacement in file |
| `Bash` | Execute shell commands (blocked commands configurable) |

## Project Structure

```
clawTao/
├── core/                    # Rust backend
│   └── src/
│       ├── main.rs          # JSON-RPC server + routing
│       ├── chat.rs          # LLM interaction loop
│       ├── config.rs        # Persistent config
│       ├── session.rs       # Session manager
│       ├── session/
│       │   ├── store.rs     # SessionStore trait
│       │   ├── json_store.rs  # JSONL implementation
│       │   └── sqlite_store.rs # SQLite implementation
│       ├── sse.rs           # SSE response parser
│       ├── jsonrpc.rs       # JSON-RPC 2.0 types
│       └── tools/           # Tool system
│           ├── spec.rs      # ToolSpec
│           ├── executor.rs  # ToolExecutor trait
│           ├── registry.rs  # ToolRegistry
│           └── builtin/     # Built-in tools
├── electron/               # Electron app
│   ├── main/index.ts       # Main process
│   ├── preload/index.ts    # contextBridge API
│   └── renderer/src/       # React UI
└── package.json
```

# ClawTao

Desktop AI Agent — Rust backend + Electron UI.

## Design

Rust backend communicates with Electron via JSON-RPC 2.0 over stdin/stdout. Each active session runs in its own thread (actor model) — concurrent sessions with per-session streaming state, non-blocking.

Config is managed by Electron (config.json + secrets.json). Rust is stateless — each `chat.send` carries its own config. LLM requests and SSE streaming are handled by protocol adapters (OpenAI / Anthropic).

### Chat Loop

Agent loop is an explicit state machine: Sampling → Evaluating → Executing → Finalizing / Interrupted. Tool calls are executed sequentially within a turn; completed results are persisted to session store. Partial results from interrupted turns are preserved as context.

### Tools

| Tool | Description |
|------|-------------|
| `Read` | Read file contents with optional offset/limit |
| `Write` | Create or overwrite a file |
| `Edit` | Exact string replacement (with optional `replace_all`) |
| `Bash` | Execute shell commands (blocked commands + timeout configurable) |
| `Grep` | Regex search with file glob filtering |
| `WebFetch` | Fetch and extract readable content from a URL |
| `WebBrowser` | Control a Chromium browser via Playwright |
| `TodoWrite` | In-turn task list, ephemeral (not persisted) |

### Features

- **Extended thinking** — real-time streaming of model reasoning (blue text, collapsible). Per-session toggle
- **Interrupt** — stop a running turn; partial content preserved, remaining tools marked as interrupted
- **Multi-provider** — built-in presets for DeepSeek and MiniMax (Anthropic protocol), plus custom provider support
- **Multi-session** — concurrent chat sessions with independent state and model selection
- **Per-session model** — each session picks its own model; new sessions inherit a configurable default
- **Internationalization** — Chinese, English, Japanese, Russian, French, Korean

## Quick Start

```bash
pnpm install
pnpm run core:build    # first time only
pnpm dev               # hot reload for UI, auto-restart for Rust
```

First launch opens Settings. Add a provider (DeepSeek / MiniMax), enter your API key, add models, and click **Test Connection**. Set a default model, then save.

## Commands

```bash
pnpm dev              # Dev mode
pnpm run core:build   # Build Rust (release)
pnpm build            # Full build (Rust + Vite + electron-builder)
pnpm test:all         # Frontend + Rust tests
pnpm test:ui          # Frontend tests (Vitest)
pnpm test:core        # Rust tests (Cargo)
```

## Configuration

Config stored in Electron's app data directory (`~/Library/Application Support/clawtao/clawtao/`):

- `config.json` — providers, models, default model, log level, bash settings
- `secrets.json` — API keys encrypted via Electron `safeStorage` (OS-level encryption)

All config is managed through the Settings UI. Rust has no config state — each `chat.send` receives config from the frontend.

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `SESSION_STORE` | `sqlite` | Session storage: `sqlite` or `json` |
| `RUST_LOG` | `clawtao=info` | Log level for the clawTao crate only |

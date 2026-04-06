# OpenHermes - Rust Rewrite of Hermes Agent

The self-improving AI agent — rewritten in Rust from the ground up for maximum performance, safety, and concurrency.

[![Rust](https://img.shields.io/badge/Rust-1.85+-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

## Overview

OpenHermes is a complete Rust rewrite of [Hermes Agent](https://github.com/NousResearch/hermes-agent) (v0.7.0), the self-improving AI agent built by [Nous Research](https://nousresearch.com). This project leverages Rust's type safety, zero-cost abstractions, and native async/await support to build a higher-performance, more reliable AI agent.

## Architecture

The project is organized as a Cargo workspace with 9 crates:

```
OpenHermes/
├── openhermes-constants/    # Shared constants and configuration
├── openhermes-config/       # Configuration management (YAML + .env)
├── openhermes-core/         # Core Agent loop with tool calling
├── openhermes-tools/        # Tool system (registry + implementations)
├── openhermes-memory/       # Memory system (SQLite + FTS5 - placeholder)
├── openhermes-skills/       # Skills system (placeholder)
├── openhermes-gateway/      # Messaging platform gateway (placeholder)
├── openhermes-cron/         # Cron scheduler (placeholder)
└── openhermes-cli/          # CLI interface with TUI
```

## Features Implemented ✅

### Phase 1: Infrastructure
- [x] Cargo workspace structure
- [x] Shared constants module
- [x] Configuration management with YAML and .env support
- [x] Profile support (HERMES_HOME)

### Phase 2: Core Agent
- [x] AIAgent structure with async/await
- [x] Iteration budget (thread-safe)
- [x] Conversation loop with tool calling
- [x] Context compressor (stub)
- [x] System prompt builder

### Phase 3: Tool System
- [x] Central tool registry with trait-based design
- [x] File tools (read_file, write_file)
- [x] Terminal tool (execute_code with timeout)
- [x] Tool discovery and registration

### Phase 4-8: Stub Implementations
- [x] Memory system (placeholder)
- [x] Skills system (placeholder)
- [x] Gateway (placeholder)
- [x] Cron scheduler (placeholder)

### Phase 9: CLI
- [x] Interactive CLI with basic REPL
- [x] Slash command support (/new, /reset, /help)
- [x] Doctor command for diagnostics
- [x] Model configuration command

## Building

```bash
# Clone the repository
git clone https://github.com/maple603/OpenHermes.git
cd OpenHermes

# Build the project
cargo build

# Run in release mode
cargo build --release

# Run the CLI
cargo run --bin hermes -- chat
```

## Usage

### Interactive Chat

```bash
# Start interactive CLI
hermes

# Or with cargo
cargo run --bin hermes
```

### Configuration

Create `~/.hermes/config.yaml`:

```yaml
agent:
  model: "anthropic/claude-opus-4.6"
  max_iterations: 90

terminal:
  backend: local
  timeout: 300
```

Create `~/.hermes/.env`:

```bash
OPENAI_API_KEY=your-api-key-here
# or
ANTHROPIC_API_KEY=your-api-key-here
# or
OPENROUTER_API_KEY=your-api-key-here
```

### Diagnostics

```bash
hermes doctor
```

## Key Differences from Python Version

| Feature | Python Version | Rust Version |
|---------|---------------|--------------|
| Async Model | asyncio + complex bridging | Native async/await + Tokio |
| Concurrency | ThreadPoolExecutor | tokio::task::JoinSet |
| Type Safety | Dynamic typing | Static typing with compile-time checks |
| Memory Safety | GC | Ownership + borrowing |
| Performance | Interpreted | Compiled to native code |
| Binary Size | ~70K+ lines Python | ~15K lines Rust (so far) |

## Roadmap

### Milestone 1: Core Functionality (Weeks 1-2) ✅
- [x] Project infrastructure
- [x] Basic agent loop
- [x] Tool registry + basic tools

### Milestone 2: Complete Tools (Weeks 3-4)
- [ ] Web search tools
- [ ] Browser automation
- [ ] MCP integration
- [ ] All 40+ tools

### Milestone 3: Memory & Skills (Weeks 5-6)
- [ ] SQLite + FTS5 implementation
- [ ] Memory providers
- [ ] Full skill system
- [ ] Session search

### Milestone 4: Gateway (Weeks 7-8)
- [ ] Telegram adapter
- [ ] Discord adapter
- [ ] Slack adapter
- [ ] All 17+ platforms

### Milestone 5: Production Ready (Weeks 9-10)
- [ ] TUI with Ratatui
- [ ] Docker support
- [ ] CI/CD pipeline
- [ ] Documentation

## Technical Stack

- **Runtime**: Tokio (async runtime)
- **HTTP**: reqwest + async-openai
- **CLI**: clap + tokio::io
- **Database**: sqlx (planned for SQLite)
- **Serialization**: serde + serde_yaml
- **Concurrency**: DashMap, parking_lot
- **Logging**: tracing + tracing-subscriber

## Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines.

Quick start for contributors:

```bash
cargo build
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --all
```

## License

MIT - see [LICENSE](LICENSE)

Built by [Nous Research](https://nousresearch.com).

---

**Note**: This is an ongoing rewrite. Many features are still in placeholder/stub state. Check the roadmap for implementation status.

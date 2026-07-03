# Contributing to Wornitor

Thanks for your interest in contributing!

## Getting Started

1. Fork the repo
2. Clone your fork
3. Install dependencies: `pnpm install`
4. Run in dev mode: `pnpm tauri dev`

## Development

### Tech Stack

- **Frontend**: React 19, TypeScript, Zustand, Recharts, Vite
- **Backend**: Rust, Tauri v2, SQLite, reqwest, tokio
- **Desktop**: Tauri v2 (multi-window, system tray)

### Code Style

- Rust: `cargo fmt` and `cargo clippy`
- TypeScript: follow existing patterns, use `pnpm format`

### Testing

```bash
# Rust tests
cd src-tauri && cargo test

# TypeScript type check
npx tsc --noEmit

# Full build
pnpm tauri build --debug
```

### Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):
```
feat: add PDF report export
fix: prevent panic on missing display
docs: update README with Windows instructions
```

## Pull Requests

1. Create a feature branch from `main`
2. Make changes + tests
3. Run `cargo test` and `npx tsc --noEmit`
4. Open PR with description of changes

## Questions?

Open a [GitHub Discussion](https://github.com/Liao-Tingfeng/Wornitor/discussions).

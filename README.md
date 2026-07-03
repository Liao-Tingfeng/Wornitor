# Wornitor

> 🖥️ AI-powered desktop activity tracker — capture, analyze, and report your work automatically.

Wornitor captures screenshots of your desktop at configurable intervals, uses LLM (OpenAI / Ollama / Kimi) to analyze what you're working on, and generates daily / weekly / monthly reports.

![License](https://img.shields.io/badge/license-MIT-blue)
![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey)
![Rust](https://img.shields.io/badge/rust-1.77%2B-orange)

## ✨ Features

- **Automatic Screenshot Capture** — Configurable intervals, multi-display support
- **AI Activity Analysis** — LLM classifies each work session (dev, meeting, design, etc.)
- **Smart Deduplication** — Perceptual hash (dHash) avoids redundant analysis
- **Idle Detection** — Skips capture when screen is locked or unchanged
- **Privacy Controls** — App blocklist, window title blocklist, local LLM support (Ollama)
- **Work Reports** — Daily / weekly / monthly summaries with charts and AI-generated summaries
- **Multi-LLM** — OpenAI, Ollama (local), Kimi Batch API, custom OpenAI-compatible providers
- **Cost Tracking** — Per-request token usage and estimated cost
- **System Tray** — Minimize to tray, quick pause/resume

## 📸 Screenshots

*(Coming soon)*

## 🚀 Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) 1.77+
- [Node.js](https://nodejs.org/) 20+
- [pnpm](https://pnpm.io/) 9+
- **macOS**: Xcode Command Line Tools
- **Windows**: Visual Studio Build Tools 2022 (C++ Desktop workload), WebView2 Runtime
- **Linux**: `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev`

### Install & Run

```bash
# Clone
git clone https://github.com/Liao-Tingfeng/Wornitor.git
cd wornitor

# Install dependencies
pnpm install

# Run in dev mode
pnpm tauri dev

# Build for production
pnpm tauri build
```

### Configure LLM

1. Launch the app
2. Go to **Settings → LLM**
3. Add your API provider:
   - **OpenAI**: `https://api.openai.com/v1`, your API key, model `gpt-4o-mini`
   - **Ollama** (local): `http://localhost:11434`, no key needed
   - **Kimi**: Custom endpoint with batch API support
4. Test connection → Save

## 🏗️ Architecture

```
┌─────────────┐     ┌─────────────────────────────────┐
│  React 19   │◄───►│  Tauri v2 (Rust)                │
│  TypeScript │ IPC │                                 │
│  Zustand    │     │  ┌──────────┐  ┌─────────────┐  │
│  Recharts   │     │  │ Scheduler│  │ LLM Adapters│  │
└─────────────┘     │  │ (tick)   │  │ OpenAI      │  │
                    │  └────┬─────┘  │ Ollama      │  │
                    │       │        │ Kimi Batch  │  │
                    │  ┌────▼─────┐  └─────────────┘  │
                    │  │ Screen   │                   │
                    │  │ (xcap)   │  ┌─────────────┐  │
                    │  └────┬─────┘  │ Database    │  │
                    │       │        │ (SQLite)    │  │
                    │       └────────┤             │  │
                    │                └─────────────┘  │
                    └─────────────────────────────────┘
```

## 📂 Project Structure

```
wornitor/
├── src/                    # Frontend (React + TypeScript)
│   ├── components/         # Timeline, Report, Settings
│   ├── stores/             # Zustand state management
│   ├── i18n/               # zh/en translations
│   └── types/              # TypeScript definitions
├── src-tauri/              # Backend (Rust)
│   ├── src/
│   │   ├── scheduler/      # Analysis loop (capture → LLM → DB)
│   │   ├── screen/         # Cross-platform screen capture (xcap)
│   │   ├── llm/            # LLM adapters (OpenAI, Ollama, Kimi)
│   │   ├── db/             # SQLite database layer
│   │   └── commands/       # Tauri IPC commands
│   └── migrations/         # Database migrations
└── package.json
```

## 🔒 Privacy & Security

- **Your data stays local**: Screenshots are stored on your machine in SQLite
- **API keys are yours**: Configured in-app, never hardcoded, never uploaded
- **Local LLM support**: Use Ollama to keep all analysis on-device
- **Privacy rules**: Block specific apps or window titles from being captured
- **Desktop permission**: macOS requires explicit Screen Recording permission

> ⚠️ **Important**: This tool is designed for **personal productivity tracking**. Do not use it to monitor others without their explicit consent. Sending screenshots to cloud LLM services (OpenAI, Kimi) means third-party data processing — use local models (Ollama) for sensitive work.

## 📄 License

MIT — see [LICENSE](./LICENSE) for details.

All dependencies are MIT or Apache-2.0 licensed. Zero GPL/AGPL copyleft.

## 🤝 Contributing

Contributions welcome! Please:

1. Open an issue to discuss before major changes
2. Follow existing code style (`cargo fmt`, `pnpm format`)
3. Add tests for new functionality
4. Update documentation as needed

## 🛣️ Roadmap

- [x] macOS support
- [x] Windows / Linux support
- [x] Multi-LLM provider support
- [ ] PDF report export
- [ ] Lark/Feishu integration
- [ ] Plugin system for custom report templates
- [ ] Mobile companion app

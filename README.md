<div align="center">
  <img src="apps/desktop/public/icon.svg" alt="RenderPilot Logo" width="128" height="128" />

  <h1>RenderPilot</h1>

  <p><strong>Manage, update, and swap PC game rendering libraries — DLSS, FSR, XeSS and more — from a single interface.</strong></p>

  <div>
    <a href="https://boosty.to/osyka.yuri/donate"><img src="https://img.shields.io/badge/☕_Support_on-Boosty-F15C22?style=for-the-badge&labelColor=1c1c1c" alt="Support on Boosty"/></a>
    <img src="https://img.shields.io/badge/License-GPLv3-4a9eff?style=for-the-badge&labelColor=1c1c1c" alt="License" />
  </div>

  <div style="margin-top: 10px;">
    <img src="https://img.shields.io/badge/Tauri-2.0-24c8db?style=for-the-badge&logo=tauri&logoColor=white&labelColor=1c1c1c" alt="Tauri" />
    <img src="https://img.shields.io/badge/Svelte-5.0-ff3e00?style=for-the-badge&logo=svelte&logoColor=white&labelColor=1c1c1c" alt="Svelte" />
    <img src="https://img.shields.io/badge/Rust-1.80+-ce4a00?style=for-the-badge&logo=rust&logoColor=white&labelColor=1c1c1c" alt="Rust" />
    <img src="https://img.shields.io/badge/Platform-Windows-0078d4?style=for-the-badge&logo=windows&logoColor=white&labelColor=1c1c1c" alt="Windows" />
  </div>
</div>

<br />

<!-- 💡 Add screenshot or GIF here -->
<!-- <div align="center"><img src="docs/screenshot.png" alt="RenderPilot Screenshot" width="80%" /></div> -->

RenderPilot automatically scans your installed games, identifies which upscaler libraries they use, and lets you upgrade, downgrade, or swap them in one click. All processing happens locally — no telemetry, no cloud accounts required.

## ✨ Features

- **🔍 Automatic Game Detection:** Scans your system and recognizes DLSS, FSR, XeSS, DirectStorage, and related libraries across all detected titles.
- **🔄 One-Click Library Management:** Upgrade to the latest version or roll back to a previous one without touching game files manually.
- **📦 Centralized Catalog:** Browse available library versions pulled from a continuously updated manifest.
- **🛡️ Safe Rollback:** Original files are backed up before any modification — restore any game to its original state at any time.
- **💾 Local-First:** Everything stored in a local SQLite database. Fast, private, and fully offline-capable.
- **⚡ Native Performance:** Built on Tauri and Rust — tiny binary, instant startup, minimal memory footprint.

## 🛠️ Supported Technologies

| Vendor | Technologies |
| :---: | :--- |
| <img src="https://img.shields.io/badge/NVIDIA-76b900?style=flat-square&logo=nvidia&logoColor=white" alt="NVIDIA" /> | DLSS Super Resolution · Frame Generation · Ray Reconstruction · Streamline · Reflex |
| <img src="https://img.shields.io/badge/AMD-ed1c24?style=flat-square&logo=amd&logoColor=white" alt="AMD" /> | FSR (Upscaler, Loader, Radiance Cache) · FSR Frame Generation · FSR Ray Regeneration |
| <img src="https://img.shields.io/badge/Intel-0071c5?style=flat-square&logo=intel&logoColor=white" alt="Intel" /> | XeSS · XeFG · Xe Low Latency |
| <img src="https://img.shields.io/badge/Microsoft-0078d4?style=flat-square&logo=microsoft&logoColor=white" alt="Microsoft" /> | DirectStorage |

## 🚀 Getting Started

> [!IMPORTANT]
> **Windows only.** Requires DirectX 12 and WebView2 Runtime.

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (latest stable)
- [Node.js](https://nodejs.org/) v20+
- [pnpm](https://pnpm.io/installation)
- Windows C++ Build Tools and WebView2 Runtime

### Build from Source

```bash
git clone https://github.com/osyka-yuri/renderpilot.git
cd renderpilot/apps/desktop
pnpm install
pnpm tauri dev
```

## 🏗️ Architecture

RenderPilot is a Rust workspace with a Svelte 5 frontend, organized around a clean separation of domain, application, and infrastructure layers:

```text
apps/
  desktop/ui                 →  Svelte 5 frontend (Shadcn/bits-ui, Tailwind CSS)

crates/
  renderpilot-domain         →  Pure domain models, no external dependencies
  renderpilot-application    →  Core use cases and business logic
  renderpilot-detection      →  Filesystem scanning — game and library detection
  renderpilot-storage-sqlite →  SQLite persistence layer
  renderpilot-cli            →  Admin CLI for manifest generation
```

## 📄 License

Licensed under the [GNU General Public License v3.0](LICENSE.txt).

## ☕ Support

**If RenderPilot saves you time, consider supporting its development:**

<a href="https://boosty.to/osyka.yuri/donate"><img src="https://img.shields.io/badge/☕_Support_on-Boosty-F15C22?style=for-the-badge&labelColor=1c1c1c" alt="Support on Boosty"/></a>

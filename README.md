# 🌌 Entangle

**High-Performance, Low-Latency Remote Desktop Protocol.**

Built with Rust (2026 Edition), Tauri v2, and QUIC. Designed for engineers and power users who need AnyDesk/TeamViewer performance with an open-source, secure-by-default architecture.

![Entangle Vision](https://via.placeholder.com/1200x600?text=Entangle+Remote+Desktop+Cyberpunk+Interface)

---

## 🎯 Core Objective

Build a lightweight, secure, low-latency alternative to commercial remote support tools. Entangle prioritizes **Latency (RTT)** and **Frame Stability** over perfect image quality, making it ideal for technical support and remote work.

## 🧱 Project Structure

A Rust workspace with strict separation of concerns:

- `apps/desktop-client`: Tauri v2 App (Rust + React/Vite/Shadcn).
- `apps/signaling-server`: Axum-based WebSocket Signaling & Discovery.
- `crates/capture`: Platform-native screen capture (SCKit for Mac, DXGI for Win).
- `crates/encoder`: Video encoding abstraction (H.264/H.265, Hardware accelerated).
- `crates/net-transport`: QUIC networking (Unreliable Datagrams for video, Reliable Streams for input).
- `crates/input-injector`: OS-level input simulation.
- `crates/crypto-session`: E2EE (X25519 + ChaCha20Poly1305).
- `crates/shared-protocol`: Shared packet definitions and enums.

## 🛠 Tech Stack

- **Core:** Rust 1.83+ (2026 Edition)
- **Networking:** QUIC (`quinn`), WebSockets (`axum`)
- **Desktop:** Tauri v2
- **Frontend:** React + Vite + TypeScript
- **UI:** Shadcn UI + Tailwind CSS
- **Media:** WebCodecs API (Frontend), ScreenCaptureKit/DXGI (Backend)

---

## 🚦 Current Status & Roadmap (Product-Ready Path)

### ✅ Completed

- [x] High-level Workspace Architecture
- [x] Shared Protocol Definitions
- [x] Basic Signaling Server (WebSocket)
- [x] Frontend Scaffolding (React + Shadcn UI)
- [x] WebCodecs Video Decoder Hook
- [x] Tauri Command Structure

### 🚧 In Progress

- [ ] **Native Capture:** Migrating from mock frames to `ScreenCaptureKit` (macOS) and `DXGI` (Windows).
- [ ] **QUIC Transport:** Replacing mock connection logic with `quinn` P2P datagrams.
- [ ] **Hardware Encoding:** Implementing `VideoToolbox` (macOS) and `NVENC` wrappers.

### 📅 Next Steps

- [ ] **NAT Traversal:** ICE/STUN integration for P2P connectivity.
- [ ] **Dirty Rect Detection:** Intelligent frame delta encoding to save bandwidth.
- [ ] **Input Injection:** Mapping frontend mouse/kb events to `CGEvent` (macOS) and `SendInput` (Windows).

---

## 🚀 Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) (1.83+ is recommended for 2026 Edition support)
- [Node.js](https://nodejs.org/) & `pnpm`
- macOS (Xcode installed) or Windows (Visual Studio C++ Build Tools)

### Development

1.  **Start Signaling Server:**
    ```bash
    cargo run -p signaling-server
    ```
2.  **Launch Desktop Client:**
    ```bash
    pnpm install
    cargo tauri dev
    ```

---

## ⚠️ Architectural Constraints

1.  **Transport Layer:** Use **QUIC Datagrams** for video (unreliable) and **Streams** for input/control (reliable).
2.  **Rendering:** Do **NOT** decode on the CPU. Use the browser's **WebCodecs API** in the React frontend.
3.  **Capture:** Always use **Dirty Rect** detection. Only encode pixels that actually changed.

---

## 📄 License

MIT © 2026 [Parevo](https://parevo.co)

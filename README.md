# OpenPeripheral

**Open-source universal peripheral customization software.**

Customize all your peripherals — keyboards, mice, headsets, mouse pads, smart lights — from any brand, through a single unified interface. Built in Rust with a [CanvasX](https://github.com/The-Ico2/CanvasX) GPU-native UI.

## Architecture

```bash
OpenPeripheral/
├── app/                # Main application (CanvasX UI + entry point)
├── crates/
│   ├── op-core/        # Device abstraction, HID communication, profiles
│   ├── op-addon/       # Addon loading, registry, manifest system
│   ├── op-ai/          # AI-guided signal reverse engineering
│   └── op-sdk/         # SDK for addon developers
├── addons/             # Example/built-in addons
└── assets/             # UI assets (HTML, CSS for CanvasX)
```

## Key Features

- **Universal Device Support** — One app for all your peripherals, regardless of brand
- **Addon System** — Download only the brand/device addons you need; community and manufacturers can publish support packages
- **AI Signal Learning** — Guided reverse-engineering of device protocols: the AI walks you through actions while capturing and analyzing USB signals, then exports a device profile (JSON/YAML)
- **GPU-Native UI** — Built on CanvasX for a fast, beautiful interface with no browser overhead
- **Open Source** — MIT licensed, community-driven

## Supported Device Types

| Type | Examples |
| ------ | ---------- |
| Keyboards | Corsair K65 Plus Wireless |
| Mice | Logitech G Pro Superlight 2 |
| Headsets | Corsair Void |
| Mouse Pads | Corsair RGB Mouse Pads |
| Smart Lights | Govee, Philips Hue *(future)* |

## How Addons Work

Each addon is a self-contained package with:

- A **manifest** (`addon.yaml`) declaring supported vendor/product IDs
- A **shared library** (`.dll`/`.so`) implementing the `DeviceDriver` trait
- Optional **UI assets** for device-specific settings panels

Addons are loaded on demand — only the drivers for your connected devices are active.

## AI Signal Learning

For unsupported devices, OpenPeripheral's AI guide will:

1. Prompt you through a series of actions (e.g., "Change your DPI to maximum")
2. Capture raw USB/HID traffic during each action
3. Detect patterns and correlate signals with actions
4. Export a device profile describing the protocol

This makes it easy for anyone to contribute support for new devices.

## Building

```bash
cargo build --release
```

## License

MIT

<p align="center">
  <img src="src/icon.svg" width="96" alt="Dannesk">
</p>

<h1 align="center">Dannesk</h1>

<p align="center">An open-source, self-custodial wallet for XRP and Bitcoin.</p>

<p align="center">
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-2024_edition-000000?logo=rust&logoColor=white" alt="Rust"></a>
  <a href="https://iced.rs"><img src="https://iced.rs/badge.svg" alt="Made with iced"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0-3B3F43" alt="License: GPL-3.0"></a>
  <a href="https://dannesk.com"><img src="https://img.shields.io/badge/Dannesk-dannesk.com-1E2227?labelColor=272C33&logoColor=F2F4F7&logo=data:image/svg%2Bxml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjI2MiAyNjIgNTAwIDUwMCI%2BPGcgZmlsbD0iI2ZmZiIgc3Ryb2tlPSIjZmZmIiBzdHJva2Utd2lkdGg9IjI2IiBzdHJva2UtbGluZWpvaW49InJvdW5kIiBzdHJva2UtbGluZWNhcD0icm91bmQiPjxwYXRoIGQ9Ik0zMzksMzE3IEw2NTgsMzE3IEw1OTgsNDM0IEwyNzgsNDM0IFoiLz48cGF0aCBkPSJNNDI2LDU5MCBMNzQ2LDU5MCBMNjg1LDcwNyBMMzY2LDcwNyBaIi8%2BPC9nPjwvc3ZnPg%3D%3D" alt="dannesk.com"></a>
</p>

## Features

- **Self-custodial** — Users can create or import wallets.
- **Key management** — Users have full control over where their seed is stored: on-device or cold storage.
- **XRPL DEX** — Trade stablecoins directly on the XRP Ledger's decentralized exchange, with routing across order books and AMM pools.
- **Bitcoin HD wallet** — Fee tiers, custom transaction fees, rotating addresses, and Replace-by-Fee (RBF).
- **Live market data** — Prices, charts, and network data.
- **Destination Tags** Create destination tags when sending or receiving.
- **Custom dashboard** — Arrange panes to build your own workspace.
- **Light & dark themes** — Choose the interface that suits your workflow.
- **Zero platform fees** — No wallet platform fees. You pay only the applicable blockchain fees. 


## Security

- Transactions are built and signed locally; only the signed transaction blob leaves your device.
- **Encryption at rest** — On-device storage, uses AES-256-GCM encryption with Argon2id for password-based key derivation.
- **Memory protection** — Sensitive memory is zeroized and wiped.
- **Hardened builds** — Release builds use security hardening measures.

## Architecture

- **Rust** — Core application
- **iced** — Desktop GUI

## Download

Download the latest release from the project's [Releases page](https://github.com/Dannesk/app/releases).

## Building from Source

Clone the repository:

```sh
git clone https://github.com/Dannesk/app.git
cd app
```

Build with Cargo:

```sh
cargo build --release
```

Run the release build:

```sh
cargo run --release
```

## License

This project is licensed under the GNU General Public License v3.0 (GPL-3.0). See [LICENSE](LICENSE).

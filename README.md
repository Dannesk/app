<p align="center">
  <img src="src/icon.svg" width="96" alt="Dannesk">
</p>

<h1 align="center">Dannesk</h1>

<p align="center">An open-source, self-custodial wallet for XRP and Bitcoin.</p>

<p align="center">
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-2024_edition-000000?logo=rust&logoColor=white" alt="Rust"></a>
  <a href="https://iced.rs"><img src="https://iced.rs/badge.svg" alt="Made with iced"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0-3B3F43" alt="License: GPL-3.0"></a></p>

## Features

- **Self-custodial** — Create or import wallets.
- **Key management** — Full control over where the seed is stored: on-device or cold storage.
- **XRPL DEX** — Trade stablecoins directly on the XRP Ledger's decentralized exchange, with routing across order books and AMM pools.
- **Bitcoin HD wallet** — Fee tiers, custom transaction fees, rotating addresses, and Replace-by-Fee (RBF).
- **Live market data** — Prices, charts, and network data.
- **Destination Tags** — Supports destination tags when sending or receiving.
- **Custom dashboard** — Drag, Drop, and arrange panes on the dashboard.
- **Light & dark themes** — Multiple themes available.
- **Zero platform fees** — No wallet platform fees. You pay only the applicable blockchain fees. 


## Security

- **Locally signed** — Transactions are built and signed locally on the users device; only the signed transaction blob goes out to the network.
- **Encryption at rest** — On-device storage uses AES-256-GCM encryption with Argon2id for password-based key derivation.
- **Memory protection** — Sensitive memory is zeroized and wiped.
- **Hardened builds** — Release builds use security hardening measures.

## Architecture

- **Rust** — Core application
- **iced** — Desktop GUI

## Download

**apt repository** — Install and update from the command line:

```sh
sudo curl -fsSL -o /usr/share/keyrings/dannesk-archive-keyring.gpg https://apt.dannesk.com/dannesk-archive-keyring.gpg
sudo tee /etc/apt/sources.list.d/dannesk.sources > /dev/null <<EOF
Types: deb
URIs: https://apt.dannesk.com
Suites: stable
Components: main
Architectures: amd64 arm64
Signed-By: /usr/share/keyrings/dannesk-archive-keyring.gpg
EOF
sudo apt update && sudo apt install dannesk
```

**Direct download** — Get the `.deb` from [dannesk.com](https://dannesk.com) or the [Releases page](https://github.com/Dannesk/app/releases). 

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

Dannesk is licensed under the GNU General Public License v3.0 (GPL-3.0). See [LICENSE](LICENSE).

The `xrpl_codec` crate in [xrpl_codec/](xrpl_codec/) is licensed under the MIT License. See [xrpl_codec/LICENSE](xrpl_codec/LICENSE).

The `dannesk-core` crate in [dannesk-core/](dannesk-core/) is the application core, shared with the Android app, under the same GPL-3.0.

The `dannesk-noise-protocol` crate in [dannesk-noise-protocol/](dannesk-noise-protocol/) is the encrypted socket between the app and its service: the Noise NK handshake, written from the specification and verified against its published test vectors, under the same GPL-3.0.

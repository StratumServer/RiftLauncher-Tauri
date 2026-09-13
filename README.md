> [!WARNING]
> **This repository is an experiment, not a release.** It is the Tauri rewrite of the RiftLauncher host, and it is not the launcher anyone should be running. Downloads and support live at [StratumServer/RiftLauncher](https://github.com/StratumServer/RiftLauncher).

# RiftLauncher on Tauri

[![CI](https://github.com/StratumServer/RiftLauncher-Tauri/actions/workflows/ci.yml/badge.svg?branch=dev)](https://github.com/StratumServer/RiftLauncher-Tauri/actions/workflows/ci.yml)

## What this repository is

RiftLauncher's front end, running on a Tauri v2 host instead of an Electron one. The React and TypeScript under `src/renderer` is the same code the shipping launcher runs, the pure business logic under `src/domain` is the same too, and what replaces `src/main`, `src/preload` and `src/ipc` is a small Rust binary in `src-tauri`.

This exists to answer [RiftLauncher issue 18](https://github.com/StratumServer/RiftLauncher/issues/18): whether the launcher is better off on Tauri. The question worth answering is not whether a Tauri window opens, it is what the host has to grow back before the front is whole again. So the host here does three things, and everything else the front asks for is a stub that refuses out loud. `src/renderer/src/host/tauriApi.ts` is the honest inventory of the gap.

The front is shared, and it is not shared symmetrically. Fixes to anything under `src/renderer` or `src/domain` belong in [StratumServer/RiftLauncher](https://github.com/StratumServer/RiftLauncher) and get cherry-picked here afterwards. A fix that lands here first is a fix that will be lost the next time the two are reconciled. What is genuinely this repository's own is `src-tauri`, the build configuration, and `tauriApi.ts`.

## What works and what does not

The host reads and writes `config.json`, lists the installations that config names, and starts the game. That last one honours the installation's data folder, start parameters, environment variables, Mesa GL thread setting and Linux launch wrapper, and refuses any path outside the folders the config manages.

Everything else is missing: no mod scanning or installing, no downloads, no extraction or compression, no backups, no account login, no folder picker, no self-update, no custom icons or backgrounds, and nothing reaches the network. The launcher starts and its config survives a restart. It cannot yet install a version to start.

## Building it

Node 22 and a stable Rust toolchain. On Linux the host links against WebKitGTK 4.1, GTK 3, libsoup3 and librsvg, which `.github/workflows/ci.yml` lists by package name.

```
npm ci
npm run tauri:dev      # the app, against the vite dev server
npm run tauri:build    # AppImage, deb and rpm under src-tauri/target/release/bundle
```

The checks are `npm run typecheck`, `npm run lint:ci`, `npm run format:check` and `npx vitest run` on the front, and `cargo fmt --check`, `cargo clippy -- -D warnings` and `cargo test` inside `src-tauri`.

## What RiftLauncher is

RiftLauncher is an independent launcher for Vintage Story, built by the [Stratum](https://github.com/StratumServer) team. It installs multiple versions of the game, keeps separate installations with their own configs, mods and worlds, and manages backups. It works on Windows and Linux; macOS is planned and not built.

RiftLauncher is a fork of [VS Launcher](https://github.com/XurxoMF/vs-launcher) by [XurxoMF](https://github.com/XurxoMF), archived by its original author. Everything the launcher does today started there, and the [contributors page](docs/important-info/contributors.md) credits by name everyone who translated, tested and contributed to the original project.

RiftLauncher is unofficial and not affiliated with Anego Studios, the developers of [Vintage Story](https://www.vintagestory.at).

## How does RiftLauncher handle privacy?

Read the [RiftLauncher Privacy Policy](PRIVACY.md) for the data the launcher stores locally and the external services it contacts.

## Can I translate RiftLauncher to another language?

Yes. The locale files under `src/renderer/src/locales` are shared with [StratumServer/RiftLauncher](https://github.com/StratumServer/RiftLauncher), so a translation belongs there rather than here.

## Where can I ask for help?

You can ask anything you need on our [Discord server](https://discord.gg/vQm6z2urZs).

## Where can I report bugs?

A bug in the launcher itself goes on the [RiftLauncher issues](https://github.com/StratumServer/RiftLauncher/issues). A bug in this host goes on the [RiftLauncher-Tauri issues](https://github.com/StratumServer/RiftLauncher-Tauri/issues).

## Can I make a suggestion?

Yes, you can do so on our [Discord server](https://discord.gg/vQm6z2urZs).

## Can I support the project?

Financial support goes through the Stratum team's [OpenCollective](https://opencollective.com/stratum), which gives the project a transparent way to handle donations and project costs. Testing, translating and reporting bugs help just as much.

---
title: Install
description: What you can install today, and what is planned.
---

There is nothing to download yet: no releases, no installers and no App Store or Google Play listings. dropumi is pre-alpha and unaudited, and its protocol is a draft. Do not use it for sensitive files yet.

## What exists today

| Part | Status |
| --- | --- |
| `dropumi` command-line tool | Build it from source. Tested on macOS (Apple silicon) and on Linux in a container. It builds for Windows but has never been run there. |
| App for iPhone, iPad and Mac, with a Share Extension | In development. Build it from source. Tested on the iOS simulator and on a Mac, not yet on an iPhone or iPad. |
| Android, Windows and Linux apps | Planned, not built yet. |
| Browser companion | A proposal only. |

So far, dropumi has only been tested on one machine at a time. Nothing has been tested between two real devices yet.

## Build from source

```sh
git clone https://github.com/dropumi/dropumi.git
cd dropumi
cargo build --release --locked -p dropumi-cli
```

This builds the command-line tool with stable Rust (1.89 or newer). The binary is `target/release/dropumi`. The repository's README has the quick start, and `apps/apple/README.md` explains how to build the Apple app with Xcode.

## Phone and tablet

There is no App Store or Google Play listing. The iPhone and iPad app is in development: you can build it from source and install it on your own device with your own Apple developer signing, but it has not been tested on a real iPhone or iPad yet. When it starts receiving, it asks for access to the local network so it can find nearby devices. Choose Allow.

:::tip
If a device does not show up, check that both are on the same network and that guest-network isolation is off on your router.
:::

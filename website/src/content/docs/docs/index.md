---
title: Introduction
description: What dropumi is and how it works.
---

:::caution
dropumi is pre-alpha and unaudited, and its protocol (v1) is a draft. Do not use it for sensitive files yet. There are no releases yet. These docs describe the iPhone, iPad and Mac app that is in development, and they will change as the app takes shape. [Install](install/) lists what exists today.
:::

dropumi is a free, open-source, ultra-lightweight way to share files between your phones and computers. It works like AirDrop and needs no internet connection. It is built to work across platforms: today there is a command-line tool and an app for iPhone, iPad and Mac in development. Android, Windows and Linux apps are planned.

## How it works

1. **Open** dropumi on two devices that are on the same Wi-Fi or local network. Nearby devices appear on the radar.
2. **Pair once.** In **Settings** › **Pair a device**, one device waits and the other asks to pair. You check that both screens show the same 6-digit code. After that, the devices recognise each other.
3. **Send.** Tap **Send** next to the device, add files, then tap the **Send to** button. The other person accepts.

By default, a device accepts files only from devices it is paired with. A device you have not paired can send only while the receiver allows **Everyone** for 10 minutes. You then compare a 6-digit code for that one transfer, and it does not pair the devices. See [Send a file](send-a-file/) and [Visibility](visibility/).

Files travel directly between the two devices over your local network, encrypted on every connection. Nothing is uploaded to a server.

## Next steps

- [Install dropumi](install/)
- [Send a file](send-a-file/)
- [Fix a device that does not show up](troubleshooting/)

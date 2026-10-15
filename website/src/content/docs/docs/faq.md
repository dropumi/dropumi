---
title: FAQ
description: Frequently asked questions.
---

## Does dropumi need internet?

No. Both devices only need to be on the same local network.

## Are my files uploaded anywhere?

No. Files go directly from one device to the other.

## Is it free?

Yes. dropumi is free and open source under the [Mozilla Public License 2.0](../../license/).

## Which platforms can I use it on?

None has a release yet. Today there is:

- a command-line tool, built from source, tested on macOS (Apple silicon) and on Linux in a container; it builds for Windows but has never been run there;
- an app for iPhone, iPad and Mac, in development, tested on the iOS simulator and on a Mac.

Android, Windows and Linux apps are planned. A browser companion is only a proposal.

## Is it safe for sensitive files?

Not yet. dropumi is pre-alpha: its code and protocol have not been audited, and protocol v1 is a draft. Every connection is encrypted and authenticated, and devices pair by comparing a 6-digit code, but do not rely on it for sensitive data yet.

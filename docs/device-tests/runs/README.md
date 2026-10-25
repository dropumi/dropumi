# Device test runs

One file per session, copied from [`../TEMPLATE.md`](../TEMPLATE.md).

| Run | Devices | What ran |
|---|---|---|
| [2026-10-18-01](2026-10-18-01.md) | iPhone 15 (iOS 27.0.1), Mac arm64 (macOS 27.0.1) | Automated tests only: hosted and UI state tests on both, the app's core against the CLI over Wi-Fi and on the Mac; the manual matrix was not run |
| [2026-10-25-01](2026-10-25-01.md) | iPhone 15 (iOS 27.0.1), Mac arm64 (macOS 27.0.1), Galaxy A54 (Android 16) | Automated tests for the pairing redesign, and device-to-device pairing and transfers between the apps; the manual matrix was not run |
| [2026-10-26-01](2026-10-26-01.md) | Galaxy A54 (Android 16), Mac arm64 (macOS 27.0.1), iPhone 15 (iOS 27.0.1) | Automated tests for the Android one-screen pairing: JVM, instrumented and interop tests on the phone, and device-to-device pairing and transfers (Android ↔ Mac, iPhone ↔ Android); the manual matrix was not run |

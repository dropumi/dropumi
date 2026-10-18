#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Runs the ADR-0011 research matrix on real browsers, with no copying or tapping.

Research prototype, not a Dropumi feature. It starts serve.py three times (http on
localhost, http and https on this Mac's Wi-Fi address), opens probe.html and
index.html in each browser, and waits for each page to post its Result block:

- Safari, Chrome and Brave on this Mac, with every window on the built-in display.
  Chrome and Brave run from throwaway profiles that are deleted afterwards; Safari is
  driven with AppleScript (macOS asks once whether this may control Safari).
- Safari on an iPhone (--iphone UDID), opened with `devicectl`.
- Chrome on an Android phone (--android SERIAL), opened with `adb`.

WEB-1 is http://localhost (this Mac only), WEB-2 http://<Wi-Fi address>, WEB-3
https://<Wi-Fi address> with a throwaway self-signed certificate: Chrome and Brave
are started with that certificate's key pinned (--ignore-certificate-errors-spki-list),
which stands for a tester who checked the fingerprint; the click-through itself is a
manual test. WEB-4 connects two pages over a WebRTC DataChannel; serve.py relays the
offer and the answer, as an app serving the page could.

Results (no addresses) go to <results>/results.jsonl; a summary is printed.

Usage: python3 poc/web/run-matrix.py --results <folder> [--iphone UDID] [--android SERIAL]
"""

import argparse
import base64
import hashlib
import json
import os
import shlex
import shutil
import signal
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
SERVE = os.path.join(HERE, "serve.py")
CHROMIUM = {
    "chrome": "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "brave": "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
}
# The built-in display, in screen coordinates with the main display's top-left at 0,0.
WINDOW = None


def run(*args, check=True):
    return subprocess.run(args, capture_output=True, text=True, check=check)


def built_in_window():
    """{left, top, width, height} on the built-in display, or None without one."""
    swift = (
        "import AppKit\n"
        "let main = NSScreen.screens[0].frame\n"
        "for s in NSScreen.screens {\n"
        "  let n = s.deviceDescription[NSDeviceDescriptionKey(\"NSScreenNumber\")] as! NSNumber\n"
        "  if CGDisplayIsBuiltin(n.uint32Value) != 0 {\n"
        "    let v = s.visibleFrame\n"
        "    print(Int(v.minX), Int(main.maxY - v.maxY), Int(v.width), Int(v.height))\n"
        "  }\n"
        "}\n"
    )
    out = subprocess.run(["swift", "-"], input=swift, capture_output=True, text=True).stdout.split()
    if len(out) != 4:
        return None
    x, y, w, h = map(int, out)
    return {"left": x + 24, "top": y + 24, "width": min(1100, w - 48), "height": min(820, h - 48)}


class Servers:
    def __init__(self, results, lan, folder):
        self.procs = []
        cert, key = os.path.join(folder, "cert.pem"), os.path.join(folder, "key.pem")
        config = os.path.join(folder, "req.cnf")
        with open(config, "w", encoding="ascii") as f:
            f.write("[req]\ndistinguished_name = dn\nprompt = no\n[dn]\nCN = dropumi-research\n")
        run("openssl", "req", "-config", config, "-x509", "-newkey", "ec", "-pkeyopt",
            "ec_paramgen_curve:prime256v1", "-nodes", "-days", "1", "-addext", f"subjectAltName=IP:{lan}",
            "-keyout", key, "-out", cert)
        os.chmod(key, 0o600)
        pub = run("openssl", "x509", "-pubkey", "-noout", "-in", cert).stdout
        der = subprocess.run(["openssl", "pkey", "-pubin", "-outform", "der"], input=pub.encode(),
                             capture_output=True, check=True).stdout
        self.spki = base64.b64encode(hashlib.sha256(der).digest()).decode()
        for extra in (["--bind", "127.0.0.1", "--port", "8000"], ["--port", "8000"],
                      ["--https", "--port", "8443", "--cert", cert, "--key", key]):
            p = subprocess.Popen([sys.executable, "-I", SERVE, "--results", results, *extra],
                                 stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
            line = p.stdout.readline()
            if not line.startswith(("certificate", "serving")):
                raise SystemExit(f"error: serve.py did not start: {line.strip()}")
            self.procs.append(p)

    def stop(self):
        for p in self.procs:
            p.send_signal(signal.SIGTERM)
        for p in self.procs:
            p.wait(timeout=10)


class Mac:
    """Safari, Chrome and Brave on this Mac, windows on the built-in display."""

    def __init__(self, spki, folder):
        self.spki = spki
        self.folder = folder
        self.chromium = []

    def open(self, browser, url):
        if browser == "safari":
            w = WINDOW
            bounds = f"{{{w['left']}, {w['top']}, {w['left'] + w['width']}, {w['top'] + w['height']}}}"
            run("osascript", "-e", f'tell application "Safari" to make new document with properties {{URL:"{url}"}}',
                "-e", f'tell application "Safari" to set bounds of front window to {bounds}')
            return
        profile = tempfile.mkdtemp(prefix=f"dropumi-{browser}-", dir=self.folder)
        w = WINDOW
        p = subprocess.Popen([
            CHROMIUM[browser], f"--user-data-dir={profile}", "--no-first-run", "--no-default-browser-check",
            "--disable-sync", f"--window-position={w['left']},{w['top']}", f"--window-size={w['width']},{w['height']}",
            f"--ignore-certificate-errors-spki-list={self.spki}", "--new-window", url,
        ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.chromium.append((p, profile))

    def close(self, tag):
        run("osascript", "-e",
            f'tell application "Safari" to close (every window whose URL of current tab contains "tag={tag}")',
            check=False)
        for p, profile in self.chromium:
            p.terminate()
            try:
                p.wait(timeout=10)
            except subprocess.TimeoutExpired:
                p.kill()
            shutil.rmtree(profile, ignore_errors=True)
        self.chromium = []


def open_iphone(udid, url):
    run("xcrun", "devicectl", "device", "process", "launch", "--device", udid, "--payload-url", url,
        "com.apple.mobilesafari", check=False)


def open_android(serial, url):
    # `adb shell` runs its words through the phone's shell, where the "&" in
    # the URL would end the command: quote it. Best effort otherwise: the
    # page's own report is what counts.
    # A page cannot run while the screen is off: wake it first (this never
    # unlocks a phone with a secure lock screen; someone must keep it unlocked).
    run("adb", "-s", serial, "shell", "input", "keyevent", "KEYCODE_WAKEUP", check=False)
    run("adb", "-s", serial, "shell", "am", "start", "-a", "android.intent.action.VIEW", "-d", shlex.quote(url),
        "-p", "com.android.chrome", check=False)


def wait_for(results, tag, done, timeout):
    """The first record with this tag for which done(record) is true, or None."""
    deadline = time.monotonic() + timeout
    path = os.path.join(results, "results.jsonl")
    while time.monotonic() < deadline:
        if os.path.exists(path):
            with open(path, encoding="utf-8") as f:
                for line in f:
                    record = json.loads(line)
                    if record.get("tag") == tag and done(record):
                        return record
        time.sleep(0.5)
    return None


def main():
    global WINDOW
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--results", required=True)
    parser.add_argument("--iphone", help="UDID of an iPhone on the same Wi-Fi")
    parser.add_argument("--android", help="adb serial of an Android phone on the same Wi-Fi")
    parser.add_argument("--only", help="comma-separated scenario prefixes, for example WEB-3,WEB-4")
    args = parser.parse_args()
    os.makedirs(args.results, exist_ok=True)
    lan = run("ipconfig", "getifaddr", "en0").stdout.strip()
    WINDOW = built_in_window()
    if WINDOW is None:
        raise SystemExit("error: no built-in display; this runner only opens windows there")
    folder = tempfile.mkdtemp(prefix="dropumi-matrix-")
    servers = Servers(args.results, lan, folder)
    mac = Mac(servers.spki, folder)
    stamp = time.strftime("%H%M%S")
    summary = []

    only = [p for p in (args.only or "").split(",") if p]

    def wanted(name):
        return not only or any(name.startswith(p) for p in only)

    def probe(row, where, browser, base):
        if not wanted(f"{row}-{where}-{browser}"):
            return
        tag = f"{row}-{where}-{browser}-{stamp}"
        url = f"{base}/?auto=1&tag={tag}"
        if where == "mac":
            mac.open(browser, url)
        elif where == "iphone":
            open_iphone(args.iphone, url)
        else:
            open_android(args.android, url)
        record = wait_for(args.results, tag, lambda r: "speed" in r, 90)
        if where == "mac":
            mac.close(tag)
        c = (record or {}).get("capabilities", {})
        summary.append((row, f"{where} {browser}", "result" if record else "no result in 90 s",
                        c.get("isSecureContext"), c.get("subtle"), (record or {}).get("speed")))

    def pair(name, offer, answer):
        if not wanted(f"WEB-4-{name}"):
            return
        tag = f"web4{name}{stamp}"
        base = f"http://{lan}:8000/index.html"
        for side, (where, browser) in (("answer", answer), ("offer", offer)):
            url = f"{base}?auto={side}&room={tag}&mb=64&tag={tag}-{side}"
            if where == "mac":
                mac.open(browser, url)
            elif where == "iphone":
                open_iphone(args.iphone, url)
            else:
                open_android(args.android, url)
        record = wait_for(args.results, f"{tag}-offer",
                          lambda r: "sentBytes" in r or "errors" in r or r.get("connectionState") == "failed", 150)
        mac.close(f"{tag}-")
        summary.append(("WEB-4", f"{offer[0]} {offer[1]} -> {answer[0]} {answer[1]}",
                        "result" if record else "no result in 150 s",
                        (record or {}).get("connectionState"), (record or {}).get("selectedPair"),
                        {k: (record or {}).get(k) for k in ("sentBytes", "seconds", "mibPerSecond", "errors")}))

    try:
        for b in ("safari", "chrome", "brave"):
            probe("WEB-1", "mac", b, "http://localhost:8000")
        for b in ("safari", "chrome", "brave"):
            probe("WEB-2", "mac", b, f"http://{lan}:8000")
        if args.iphone:
            probe("WEB-2", "iphone", "safari", f"http://{lan}:8000")
        if args.android:
            probe("WEB-2", "android", "chrome", f"http://{lan}:8000")
        for b in ("chrome", "brave"):
            probe("WEB-3", "mac", b, f"https://{lan}:8443")
        pair("macchromemacsafari", ("mac", "chrome"), ("mac", "safari"))
        if args.iphone:
            pair("macchromeiphone", ("mac", "chrome"), ("iphone", "safari"))
        if args.android:
            pair("macchromeandroid", ("mac", "chrome"), ("android", "chrome"))
        if args.iphone and args.android:
            pair("iphoneandroid", ("iphone", "safari"), ("android", "chrome"))
    finally:
        mac.close(stamp)
        servers.stop()
        shutil.rmtree(folder, ignore_errors=True)
    for row in summary:
        print(" | ".join(str(x) for x in row))


if __name__ == "__main__":
    main()

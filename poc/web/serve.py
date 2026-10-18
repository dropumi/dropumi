#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Research server for ADR-0011 (browser companion): a page served by the app.

Not a Dropumi feature, not secure, never deployed. It stands in for "the app
serves a page on the LAN" so that real browsers can be measured: it serves
the two research pages of poc/web, discards uploads and streams bounded
downloads, over plain HTTP or, with --https, over TLS with a throwaway
self-signed certificate that lives in a temporary folder for one run.

Usage (from the repository root, on the Mac):
  python3 poc/web/serve.py                     # http://<LAN address>:8000
  python3 poc/web/serve.py --bind 127.0.0.1    # http://localhost:8000 (WEB-1)
  python3 poc/web/serve.py --https             # https://<LAN address>:8443 (WEB-3)
  ... --results <folder>                         # also save each page's Result block

It uses only the Python standard library and the `openssl` command, and it
makes no network request of its own. Its access log goes to the terminal
only; never paste it anywhere (it contains the clients' addresses).
"""

import argparse
import http.server
import json
import os
import shutil
import signal
import ssl
import subprocess
import sys
import tempfile
import threading
import urllib.parse

ROOT = os.path.dirname(os.path.abspath(__file__))
PAGES = {"/": "probe.html", "/probe.html": "probe.html", "/index.html": "index.html"}
MIB = 1024 * 1024
MAX_MIB = 512
CHUNK = os.urandom(MIB)  # random, so nothing on the way can compress it
# WebRTC signalling for the pages' auto mode: (room, "offer"|"answer") -> SDP.
# In memory, bounded; the app that serves a page could relay the same way.
SIGNALS = {}
SIGNALS_LOCK = threading.Lock()
MAX_SIGNALS = 64
MAX_SDP = 64 * 1024


class Handler(http.server.BaseHTTPRequestHandler):
    server_version = "dropumi-research"
    sys_version = ""

    def do_GET(self):
        url = urllib.parse.urlsplit(self.path)
        if url.path in PAGES:
            with open(os.path.join(ROOT, PAGES[url.path]), "rb") as page:
                body = page.read()
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(body)
        elif url.path == "/signal":
            key = self.signal_key(url)
            if key is None:
                return
            with SIGNALS_LOCK:
                sdp = SIGNALS.get(key)
            if sdp is None:
                self.send_response(204)
                self.send_header("Cache-Control", "no-store")
                self.end_headers()
                return
            body = sdp.encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/plain; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(body)
        elif url.path == "/download":
            mib = self.mib(url)
            if mib is None:
                return
            self.send_response(200)
            self.send_header("Content-Type", "application/octet-stream")
            self.send_header("Content-Length", str(mib * MIB))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            for _ in range(mib):
                self.wfile.write(CHUNK)
        else:
            self.send_error(404)

    def do_POST(self):
        url = urllib.parse.urlsplit(self.path)
        path = url.path
        if path == "/result":
            self.save_result()
            return
        if path == "/signal":
            key = self.signal_key(url)
            if key is None:
                return
            try:
                length = int(self.headers.get("Content-Length", ""))
            except ValueError:
                length = -1
            if not 0 < length <= MAX_SDP:
                self.send_error(400)
                return
            sdp = self.rfile.read(length).decode("utf-8", "replace")
            with SIGNALS_LOCK:
                if key not in SIGNALS and len(SIGNALS) >= MAX_SIGNALS:
                    SIGNALS.pop(next(iter(SIGNALS)))
                SIGNALS[key] = sdp
            self.send_response(204)
            self.end_headers()
            return
        if path != "/upload":
            self.send_error(404)
            return
        try:
            length = int(self.headers.get("Content-Length", ""))
        except ValueError:
            self.send_error(411, "Content-Length required")
            return
        if length < 0 or length > MAX_MIB * MIB:
            self.send_error(413, f"at most {MAX_MIB} MiB")
            return
        left = length
        while left > 0:
            got = self.rfile.read(min(left, MIB))
            if not got:
                break
            left -= len(got)
        body = json.dumps({"bytes": length - left}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def save_result(self):
        """A page's Result block (no addresses; see the pages), one JSON
        object per line in --results. Without --results it is refused."""
        folder = self.server.results
        try:
            length = int(self.headers.get("Content-Length", ""))
        except ValueError:
            length = -1
        if folder is None or not 0 < length <= 64 * 1024:
            self.send_error(400)
            return
        try:
            record = json.loads(self.rfile.read(length))
        except ValueError:
            self.send_error(400)
            return
        if not isinstance(record, dict):
            self.send_error(400)
            return
        with open(os.path.join(folder, "results.jsonl"), "a", encoding="utf-8") as out:
            out.write(json.dumps(record, sort_keys=True) + "\n")
        self.send_response(204)
        self.end_headers()

    def signal_key(self, url):
        query = urllib.parse.parse_qs(url.query)
        room = query.get("room", [""])[0]
        role = query.get("role", [""])[0]
        if not (0 < len(room) <= 64 and room.isalnum() and role in ("offer", "answer")):
            self.send_error(400)
            return None
        return (room, role)

    def mib(self, url):
        query = urllib.parse.parse_qs(url.query)
        try:
            mib = int(query.get("mib", ["16"])[0])
        except ValueError:
            mib = -1
        if not 1 <= mib <= MAX_MIB:
            self.send_error(400, f"mib must be 1 to {MAX_MIB}")
            return None
        return mib


def lan_address():
    """The Mac's Wi-Fi IPv4 address (en0), or None."""
    try:
        out = subprocess.run(["ipconfig", "getifaddr", "en0"], capture_output=True, text=True, check=True)
    except (OSError, subprocess.CalledProcessError):
        return None
    return out.stdout.strip() or None


def openssl(*args):
    """Runs the openssl command; its own message is shown if it fails."""
    run = subprocess.run(["openssl", *args], capture_output=True, text=True)
    if run.returncode != 0:
        sys.exit(f"error: openssl {args[0]} failed:\n{run.stderr.strip()}")
    return run.stdout.strip()


def self_signed(folder, address):
    """A throwaway EC key and a one-day certificate for `address`."""
    key = os.path.join(folder, "key.pem")
    cert = os.path.join(folder, "cert.pem")
    # An explicit configuration: some openssl installs ship without a default one.
    config = os.path.join(folder, "req.cnf")
    with open(config, "w", encoding="ascii") as f:
        f.write("[req]\ndistinguished_name = dn\nprompt = no\n[dn]\nCN = dropumi-research\n")
    openssl("req", "-config", config, "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1",
            "-nodes", "-days", "1", "-addext", f"subjectAltName=IP:{address}", "-keyout", key, "-out", cert)
    os.chmod(key, 0o600)
    return cert, key, openssl("x509", "-noout", "-fingerprint", "-sha256", "-in", cert)


def stop(*_):
    raise KeyboardInterrupt


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--bind", help="address to listen on (default: the Wi-Fi address)")
    parser.add_argument("--port", type=int, help="port (default 8000, or 8443 with --https)")
    parser.add_argument("--https", action="store_true", help="TLS with a throwaway self-signed certificate")
    parser.add_argument("--results", help="folder where the pages' Result blocks are saved (results.jsonl)")
    parser.add_argument("--cert", help="with --https: use this certificate (PEM) instead of a new one")
    parser.add_argument("--key", help="with --https: the key for --cert (PEM)")
    args = parser.parse_args()

    address = args.bind or lan_address()
    if not address:
        sys.exit("error: no Wi-Fi address on en0; pass --bind")
    port = args.port or (8443 if args.https else 8000)
    server = http.server.ThreadingHTTPServer((address, port), Handler)
    server.daemon_threads = True
    server.results = None
    if args.results:
        os.makedirs(args.results, exist_ok=True)
        server.results = args.results

    # A TERM stops the server like Ctrl-C, so the throwaway key is always removed.
    signal.signal(signal.SIGTERM, stop)
    folder = None
    scheme = "http"
    try:
        if args.https:
            if args.cert and args.key:
                cert, key = args.cert, args.key
                fingerprint = openssl("x509", "-noout", "-fingerprint", "-sha256", "-in", cert)
            else:
                folder = tempfile.mkdtemp(prefix="dropumi-research-")
                cert, key, fingerprint = self_signed(folder, address)
            context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            context.load_cert_chain(cert, key)
            server.socket = context.wrap_socket(server.socket, server_side=True)
            scheme = "https"
            print(f"certificate {fingerprint}", flush=True)
        host = "localhost" if address == "127.0.0.1" else address
        print(f"serving {scheme}://{host}:{port}/ (probe) and /index.html (WebRTC); Ctrl-C stops", flush=True)
        print("research prototype: not secure, not a Dropumi feature; never paste this log", flush=True)
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
        if folder:
            shutil.rmtree(folder, ignore_errors=True)


if __name__ == "__main__":
    main()

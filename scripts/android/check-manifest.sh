#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# Audits the app's merged manifests (ADR-0013 D1, D3, D8; plan §2.6, §9):
# - the requested permissions are exactly the allowlist below, and the only
#   permission the app declares is androidx.core's signature-level
#   <applicationId>.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION;
# - there is no <queries> element (the Tethering module entry comes with the
#   first API 33+ entry on the R21 gate's allowlist, HostLabelGate.kt);
# - the exported components are exactly MainActivity and ShareActivity (a
#   component with an intent filter and no android:exported counts as
#   exported); there is no receiver, the only service is TransferService,
#   and the only providers are the FileProvider and androidx.startup's
#   InitializationProvider, whose initializers are exactly the allowlist
#   below (no EmojiCompatInitializer);
# - no activity can show over the lock screen (showWhenLocked, turnScreenOn);
# - <application> has allowBackup="false" and both backup rule files, and
#   release and qa are not debuggable.
# A dependency update that adds a permission, a component or an
# initializer fails here until it is reviewed and allowlisted.
#
# Usage: bash scripts/android/check-manifest.sh [VARIANT | MANIFEST.xml]...
#   A VARIANT (release, qa, debug; the default is all three) is read from
#   apps/android/app/build/intermediates/merged_manifests after a build.
#   A path ending in .xml is audited as a release-type manifest.
# Needs python3. Exit status: 0 when every manifest passes, 1 otherwise.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
merged="${root}/apps/android/app/build/intermediates/merged_manifests"
[[ $# -gt 0 ]] || set -- release qa debug

status=0
for arg in "$@"; do
  if [[ "${arg}" == *.xml ]]; then
    manifest="${arg}"
    variant=release
  else
    variant="${arg}"
    case "${variant}" in release | qa | debug) ;; *) echo "error: unknown variant ${variant}" >&2; exit 2 ;; esac
    manifest="$(find "${merged}/${variant}" -name AndroidManifest.xml 2>/dev/null | head -n 1 || true)"
    if [[ -z "${manifest}" ]]; then
      echo "FAIL ${variant}: no merged manifest under ${merged#"${root}/"}/${variant}; build the ${variant} APK first"
      status=1
      continue
    fi
  fi
  if ! python3 -I - "${manifest}" "${variant}" <<'PY'
import sys
import xml.etree.ElementTree as ET

path, variant = sys.argv[1], sys.argv[2]
A = "{http://schemas.android.com/apk/res/android}"
problems = []

root = ET.parse(path).getroot()
pkg = root.get("package") or ""
if not pkg.startswith("io.github.dropumi"):
    problems.append(f"unexpected package {pkg!r}")
dynamic = f"{pkg}.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION"

# The permission allowlist (plan §2.6, PR 2c-1).
allowed = {
    "android.permission.INTERNET",
    "android.permission.ACCESS_NETWORK_STATE",
    "android.permission.FOREGROUND_SERVICE",
    "android.permission.FOREGROUND_SERVICE_DATA_SYNC",
    "android.permission.WAKE_LOCK",
    "android.permission.POST_NOTIFICATIONS",
    "android.permission.HIDE_OVERLAY_WINDOWS",
    dynamic,
}
requested = [e.get(A + "name") for tag in ("uses-permission", "uses-permission-sdk-23") for e in root.iter(tag)]
for name in sorted(set(requested) - allowed):
    problems.append(f"unexpected permission {name}")
for name in sorted(allowed - set(requested)):
    problems.append(f"missing permission {name}")
declared = {e.get(A + "name"): e.get(A + "protectionLevel") for e in root.iter("permission")}
if declared != {dynamic: "signature"}:
    problems.append(f"declared permissions {declared}, expected only {dynamic} (signature)")
for tag in ("permission-group", "permission-tree"):
    if root.find(tag) is not None:
        problems.append(f"unexpected <{tag}>")

if root.find("queries") is not None:
    problems.append("unexpected <queries> (none is allowlisted yet)")

app = root.find("application")
if app is None:
    problems.append("no <application>")
    app = ET.Element("application")
expect_app = {
    "allowBackup": "false",
    "dataExtractionRules": "@xml/data_extraction_rules",
    "fullBackupContent": "@xml/backup_rules",
}
for attr, value in expect_app.items():
    if app.get(A + attr) != value:
        problems.append(f"<application> android:{attr} is {app.get(A + attr)!r}, expected {value!r}")
if variant in ("release", "qa") and app.get(A + "debuggable") == "true":
    problems.append(f"{variant} is debuggable")

NAMESPACE = "io.github.dropumi"

def short(name):
    return name[len(NAMESPACE) + 1:] if name.startswith(NAMESPACE + ".") else name

kinds = ("activity", "activity-alias", "service", "receiver", "provider")
exported = set()
by_kind = {kind: [] for kind in kinds}
for kind in kinds:
    for e in app.iter(kind):
        name = e.get(A + "name") or ""
        if name.startswith("."):
            name = NAMESPACE + name
        by_kind[kind].append(name)
        flag = e.get(A + "exported")
        if flag == "true" or (flag is None and e.find("intent-filter") is not None):
            exported.add(name)
        if kind == "activity":
            for attr in ("showWhenLocked", "turnScreenOn"):
                if e.get(A + attr) == "true":
                    problems.append(f"{short(name)} sets android:{attr}")

expected_exported = {"io.github.dropumi.MainActivity", "io.github.dropumi.ShareActivity"}
if exported != expected_exported:
    problems.append(f"exported components {sorted(exported)}, expected {sorted(expected_exported)}")
if by_kind["activity-alias"]:
    problems.append(f"unexpected activity aliases {by_kind['activity-alias']}")
if by_kind["receiver"]:
    problems.append(f"unexpected receivers {by_kind['receiver']} (ProfileInstallReceiver must stay removed)")
if by_kind["service"] != ["io.github.dropumi.TransferService"]:
    problems.append(f"services {by_kind['service']}, expected only io.github.dropumi.TransferService")
expected_providers = ["androidx.core.content.FileProvider", "androidx.startup.InitializationProvider"]
if sorted(by_kind["provider"]) != expected_providers:
    problems.append(f"providers {sorted(by_kind['provider'])}, expected {expected_providers}")

# androidx.startup initializers: no EmojiCompat (it can ask the Play
# Services font provider for a font), and nothing new without review.
allowed_initializers = {
    "androidx.lifecycle.ProcessLifecycleInitializer",
    "androidx.profileinstaller.ProfileInstallerInitializer",
}
for e in app.iter("provider"):
    if e.get(A + "name") == "androidx.startup.InitializationProvider":
        initializers = {m.get(A + "name") for m in e.iter("meta-data") if m.get(A + "value") == "androidx.startup"}
        if initializers != allowed_initializers:
            problems.append(f"startup initializers {sorted(initializers)}, expected {sorted(allowed_initializers)}")

label = f"{variant} ({pkg})"
if problems:
    for p in problems:
        print(f"FAIL {label}: {p}")
    sys.exit(1)
print(f"ok   {label}: {len(set(requested))} permissions, exported {sorted(short(n) for n in exported)}, "
      f"providers {len(by_kind['provider'])}, no receivers, no <queries>, backup rules set")
PY
  then
    status=1
  fi
done
exit "${status}"

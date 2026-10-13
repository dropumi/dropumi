#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# Architecture guard for the Dropumi workspace (ADR-0001, ADR-0010).
#
# Dependencies must point inward:
#   dropumi-domain <- dropumi-protocol <- dropumi-application <- dropumi-infra <- dropumi-cli
#
# For each inner crate this script lists its resolved normal (non-dev, non-build)
# dependency graph for every target platform with all features enabled, and fails
# if a denied crate appears anywhere in it, directly or transitively. It also fails
# if `ring` appears anywhere in the workspace normal dependency graph (snow is built
# with its pure-Rust features only; ADR-0002).
#
# Usage: bash scripts/ci/check-architecture.sh
# Exit status: 0 = clean, 1 = violation(s) found, 2 = could not inspect the graph.
#
# Compatible with bash 3.2 (macOS /bin/bash): no associative arrays, no mapfile.

set -euo pipefail
set -f # no pathname expansion: deny-lists are split on whitespace only

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
cd "${repo_root}"

nl=$'\n'
violations=0

# Deny-lists: a space-separated list of crate names per package.
deny_domain="tokio tokio-util snow sha2 getrandom minicbor blake3 mdns-sd tempfile serde serde_json dirs clap tracing tracing-subscriber rustix dropumi-protocol dropumi-application dropumi-infra dropumi-cli"
deny_protocol="tokio tokio-util mdns-sd tempfile serde serde_json dirs clap tracing tracing-subscriber dropumi-application dropumi-infra dropumi-cli"
deny_application="mdns-sd tempfile dirs serde serde_json clap tracing-subscriber rustix dropumi-infra dropumi-cli"

report() {
  violations=$((violations + 1))
  if [[ "${GITHUB_ACTIONS:-}" == "true" ]]; then
    printf '::error title=architecture::%s\n' "$1"
  else
    printf 'error: %s\n' "$1" >&2
  fi
}

die() {
  if [[ "${GITHUB_ACTIONS:-}" == "true" ]]; then
    printf '::error title=architecture::%s\n' "$1"
  else
    printf 'error: %s\n' "$1" >&2
  fi
  exit 2
}

# Print the de-duplicated crate names (first whitespace-separated field of each
# `{p}` line, e.g. "tokio v1.53.2 (*)" -> "tokio") of a cargo tree selection.
# Arguments: the package selection (`-p <pkg>` or `--workspace`).
crate_names() {
  cargo tree "$@" -e normal --all-features --target all --locked --prefix none --format '{p}' \
    | awk 'NF > 0 { print $1 }' \
    | sort -u
}

# Succeeds when crate name $2 is one line of the newline-separated list $1.
# Uses a case pattern rather than `grep -q` in a pipeline, which can report a
# false negative under `pipefail` when grep exits early (SIGPIPE).
contains() {
  case "${nl}$1${nl}" in
    *"${nl}$2${nl}"*) return 0 ;;
    *) return 1 ;;
  esac
}

# check_package <package> <space-separated deny-list>
check_package() {
  local pkg="$1"
  local deny_list="$2"
  local names denied count=0

  if ! names="$(crate_names -p "${pkg}")"; then
    die "cargo tree failed for package '${pkg}'"
  fi
  # Guard against silently passing on unexpected output: the package itself is
  # always the root of its own tree.
  if ! contains "${names}" "${pkg}"; then
    die "package '${pkg}' is missing from its own cargo tree output; cannot verify its dependencies"
  fi

  # Word splitting of the deny-list is intended (globbing is disabled by set -f).
  for denied in ${deny_list}; do
    count=$((count + 1))
    if contains "${names}" "${denied}"; then
      report "architecture violation: '${pkg}' depends on '${denied}' (normal dependency, any target, all features). Inspect with: cargo tree -p ${pkg} -e normal --all-features --target all -i ${denied}"
    fi
  done
  printf 'checked %s against %d denied crates\n' "${pkg}" "${count}"
}

check_package dropumi-domain "${deny_domain}"
check_package dropumi-protocol "${deny_protocol}"
check_package dropumi-application "${deny_application}"

if ! workspace_names="$(crate_names --workspace)"; then
  die "cargo tree failed for the workspace"
fi
if ! contains "${workspace_names}" "dropumi-domain"; then
  die "workspace cargo tree output does not list dropumi-domain; cannot verify the workspace graph"
fi
if contains "${workspace_names}" "ring"; then
  report "banned crate 'ring' is in the workspace normal dependency graph (snow must use its pure-Rust features only; ADR-0002). Inspect with: cargo tree --workspace -e normal --all-features --target all -i ring"
fi
printf 'checked workspace for ring\n'

if [[ "${violations}" -gt 0 ]]; then
  printf 'architecture check FAILED: %d violation(s)\n' "${violations}" >&2
  exit 1
fi
printf 'architecture check passed\n'

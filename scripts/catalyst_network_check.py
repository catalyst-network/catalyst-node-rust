#!/usr/bin/env python3
"""
Query every Catalyst JSON-RPC endpoint in a config file and compare heads.

Uses only the standard library (no pip install). Run from your dev machine:

  python3 scripts/catalyst_network_check.py --config scripts/catalyst_network_check.example.json

Exit codes:
  0  All nodes report the same applied_state_root at the same applied_cycle (within --max-cycle-diff).
  1  Mismatch, unreachable node, or JSON-RPC error.
  2  Bad arguments or config.

  --fail-state-file PATH
      On failure: overwrite this file with UTC time + which nodes diverged or errored.
      On success: delete this file if it exists (so "file missing" means last run was OK).

RPC methods: catalyst_head, catalyst_peerCount, catalyst_genesisHash (chain identity check).
"""

from __future__ import annotations

import argparse
import csv
import json
import os
import ssl
import sys
import urllib.error
import urllib.request
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any


@dataclass
class Endpoint:
    name: str
    url: str


def load_config(path: str) -> tuple[str | None, list[Endpoint]]:
    with open(path, encoding="utf-8") as f:
        raw = json.load(f)
    network = raw.get("network")
    eps = raw.get("endpoints")
    if not isinstance(eps, list) or not eps:
        raise ValueError('config must contain a non-empty "endpoints" array')
    out: list[Endpoint] = []
    for i, e in enumerate(eps):
        if not isinstance(e, dict):
            raise ValueError(f"endpoints[{i}] must be an object")
        name = e.get("name")
        url = e.get("url")
        if not name or not url:
            raise ValueError(f"endpoints[{i}] needs \"name\" and \"url\"")
        out.append(Endpoint(name=str(name), url=str(url).rstrip("/")))
    return (str(network) if network else None), out


def rpc_call(url: str, method: str, params: list[Any] | None, timeout: float) -> Any:
    body = json.dumps(
        {"jsonrpc": "2.0", "id": 1, "method": method, "params": params or []}
    ).encode("utf-8")
    req = urllib.request.Request(
        url,
        data=body,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    ctx = ssl.create_default_context()
    with urllib.request.urlopen(req, timeout=timeout, context=ctx) as resp:
        payload = json.loads(resp.read().decode("utf-8"))
    if "error" in payload and payload["error"]:
        err = payload["error"]
        raise RuntimeError(err.get("message", str(err)))
    return payload.get("result")


def fetch_node(ep: Endpoint, timeout: float) -> dict[str, Any]:
    row: dict[str, Any] = {"name": ep.name, "url": ep.url, "error": None}
    try:
        head = rpc_call(ep.url, "catalyst_head", [], timeout)
        peers = rpc_call(ep.url, "catalyst_peerCount", [], timeout)
        genesis = rpc_call(ep.url, "catalyst_genesisHash", [], timeout)
        row["applied_cycle"] = head.get("applied_cycle")
        row["applied_state_root"] = head.get("applied_state_root")
        row["applied_lsu_hash"] = head.get("applied_lsu_hash")
        row["peer_count"] = peers
        row["genesis_hash"] = genesis
    except Exception as e:
        row["error"] = str(e)
    return row


def rows_aligned(rows: list[dict[str, Any]], max_cycle_diff: int) -> tuple[bool, str]:
    ok_rows = [r for r in rows if not r.get("error")]
    if not ok_rows:
        return False, "no successful responses"
    geneses = [r["genesis_hash"] for r in ok_rows if r.get("genesis_hash")]
    if len(set(geneses)) > 1:
        return False, f"genesis_hash differs across nodes: {geneses!r}"
    cycles = [r["applied_cycle"] for r in ok_rows if r.get("applied_cycle") is not None]
    if not cycles:
        return False, "missing applied_cycle"
    c_min, c_max = min(cycles), max(cycles)
    if c_max - c_min > max_cycle_diff:
        return (
            False,
            f"applied_cycle spread {c_min}..{c_max} exceeds max_cycle_diff={max_cycle_diff}",
        )
    # Fork: two nodes at the same cycle must agree on state_root. Lagging (lower cycle) is OK.
    by_cycle: dict[int, list[str]] = {}
    for r in ok_rows:
        c = r.get("applied_cycle")
        sr = r.get("applied_state_root")
        if c is None or not sr:
            continue
        by_cycle.setdefault(int(c), []).append(str(sr))
    for c, roots in sorted(by_cycle.items()):
        uniq = set(roots)
        if len(uniq) > 1:
            return False, f"fork: applied_state_root differs at cycle {c} ({len(uniq)} values)"
    return True, "aligned (same root per cycle; lower cycles may be lagging)"


def describe_failure_nodes(
    rows: list[dict[str, Any]],
    aligned: bool,
    summary_msg: str,
    n_ok: int,
    n_total: int,
) -> list[str]:
    """Human-readable lines naming problematic nodes."""
    lines: list[str] = []
    for r in rows:
        if r.get("error"):
            lines.append(
                f"  - {r['name']}: RPC error — {r['error']}"
            )

    if "genesis_hash differs" in summary_msg:
        gh_by: dict[str, list[str]] = {}
        for r in rows:
            if not r.get("error") and r.get("genesis_hash"):
                gh_by.setdefault(str(r["genesis_hash"]), []).append(r["name"])
        for gh, names in sorted(gh_by.items(), key=lambda x: x[0][:16]):
            lines.append(f"  - genesis group: {', '.join(names)} -> {gh[:20]}...")

    if "applied_cycle spread" in summary_msg or "exceeds max_cycle_diff" in summary_msg:
        for r in rows:
            if not r.get("error"):
                lines.append(
                    f"  - {r['name']}: applied_cycle={r.get('applied_cycle')} state_root={(r.get('applied_state_root') or '')[:18]}..."
                )

    if summary_msg.startswith("fork:"):
        # Same cycle, different roots — list each node's root at divergent cycles.
        ok_rows = [r for r in rows if not r.get("error")]
        by_cycle: dict[int, list[tuple[str, str]]] = {}
        for r in ok_rows:
            c = r.get("applied_cycle")
            sr = r.get("applied_state_root")
            if c is None or not sr:
                continue
            by_cycle.setdefault(int(c), []).append((r["name"], str(sr)))
        for c in sorted(by_cycle.keys()):
            pairs = by_cycle[c]
            roots = set(p for _, p in pairs)
            if len(roots) > 1:
                lines.append(f"  At cycle {c} (fork):")
                for name, root in sorted(pairs):
                    lines.append(f"    - {name}: {root}")

    if not lines and not aligned:
        lines.append(f"  (see summary: {summary_msg})")
    return lines


def write_fail_state_file(
    path: str,
    network: str | None,
    summary_msg: str,
    rows: list[dict[str, Any]],
    n_ok: int,
    n_total: int,
    aligned: bool,
) -> None:
    ts = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")
    lines = [
        f"datetime: {ts}",
        f"network: {network or '(unset)'}",
        f"summary: {summary_msg}",
        f"endpoints_ok: {n_ok}/{n_total}",
        "",
        "details:",
        *describe_failure_nodes(rows, aligned, summary_msg, n_ok, n_total),
        "",
    ]
    text = "\n".join(lines)
    with open(path, "w", encoding="utf-8") as f:
        f.write(text)


def append_log(log_path: str, network: str | None, rows: list[dict[str, Any]], aligned: bool) -> None:
    ts = datetime.now(timezone.utc).isoformat()
    fieldnames = [
        "timestamp_utc",
        "network",
        "aligned",
        "name",
        "url",
        "applied_cycle",
        "applied_state_root",
        "peer_count",
        "genesis_hash",
        "error",
    ]
    file_exists = False
    try:
        with open(log_path, encoding="utf-8"):
            file_exists = True
    except FileNotFoundError:
        pass
    with open(log_path, "a", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=fieldnames)
        if not file_exists:
            w.writeheader()
        for r in rows:
            w.writerow(
                {
                    "timestamp_utc": ts,
                    "network": network or "",
                    "aligned": aligned,
                    "name": r.get("name", ""),
                    "url": r.get("url", ""),
                    "applied_cycle": r.get("applied_cycle", ""),
                    "applied_state_root": r.get("applied_state_root", ""),
                    "peer_count": r.get("peer_count", ""),
                    "genesis_hash": r.get("genesis_hash", ""),
                    "error": r.get("error", ""),
                }
            )


def main() -> int:
    p = argparse.ArgumentParser(description="Compare Catalyst RPC heads across nodes.")
    p.add_argument(
        "--config",
        "-c",
        default="scripts/catalyst_network_check.example.json",
        help="Path to JSON config (see example file)",
    )
    p.add_argument(
        "--max-cycle-diff",
        type=int,
        default=2,
        help="Max allowed difference in applied_cycle between nodes (default: 2)",
    )
    p.add_argument(
        "--timeout",
        type=float,
        default=15.0,
        help="HTTP timeout seconds per request (default: 15)",
    )
    p.add_argument(
        "--log",
        metavar="FILE",
        help="Append CSV rows to FILE for history (one row per node per run)",
    )
    p.add_argument(
        "--fail-state-file",
        metavar="FILE",
        help="On failure: overwrite with UTC time + which nodes diverged. On success: delete file.",
    )
    args = p.parse_args()

    try:
        network, endpoints = load_config(args.config)
    except (OSError, ValueError, json.JSONDecodeError) as e:
        print(f"config error: {e}", file=sys.stderr)
        return 2

    rows = [fetch_node(ep, args.timeout) for ep in endpoints]

    # Table
    print(f"Network: {network or '(not set in config)'}")
    print()
    hdr = f"{'name':<12} {'cycle':>14} {'peers':>6} {'state_root':<66} {'error':<20}"
    print(hdr)
    print("-" * len(hdr))
    for r in rows:
        c = r.get("applied_cycle")
        sr = (r.get("applied_state_root") or "")[:64]
        err = (r.get("error") or "")[:18]
        pc = r.get("peer_count")
        print(
            f"{r['name']:<12} {str(c):>14} {str(pc):>6} {sr:<66} {err:<20}"
        )

    errs = [(r["name"], r.get("error")) for r in rows if r.get("error")]
    if errs:
        print()
        print("Full errors (use real DNS names / IPs your dev machine can resolve):")
        for name, e in errs:
            print(f"  {name}: {e}")
        if any("Errno -2" in str(e) or "Name or service not known" in str(e) for _, e in errs):
            print(
                "  Hint: [Errno -2] usually means DNS could not resolve the hostname "
                "(fix /etc/hosts, use Vultr public hostname, VPN, or http://IP:PORT).",
                file=sys.stderr,
            )
        if any("timed out" in str(e).lower() for _, e in errs):
            print(
                "  Hint: timeouts → firewall/security group blocking your dev IP on 443/8545, "
                "wrong URL, or RPC not listening on 0.0.0.0. Try curl from dev machine; "
                f"increase --timeout (current {args.timeout}s).",
                file=sys.stderr,
            )

    n_total = len(rows)
    n_ok = sum(1 for r in rows if not r.get("error"))
    if n_ok < n_total:
        aligned, msg = (
            False,
            f"incomplete: only {n_ok}/{n_total} endpoints responded — fix timeouts/DNS/firewall before trusting alignment",
        )
    elif n_total >= 2 and n_ok < 2:
        aligned, msg = (
            False,
            "need at least 2 responding endpoints to compare heads across nodes",
        )
    else:
        aligned, msg = rows_aligned(rows, args.max_cycle_diff)

    print()
    if aligned:
        print(f"OK: {msg}")
    else:
        print(f"NOT OK: {msg}")

    if args.log:
        try:
            append_log(args.log, network, rows, aligned)
            print(f"Logged to {args.log}")
        except OSError as e:
            print(f"warning: could not write log: {e}", file=sys.stderr)

    success = aligned and n_ok == n_total
    if args.fail_state_file:
        try:
            if success:
                try:
                    os.remove(args.fail_state_file)
                except FileNotFoundError:
                    pass
                except OSError as e:
                    print(
                        f"warning: could not remove fail-state file: {e}",
                        file=sys.stderr,
                    )
            else:
                write_fail_state_file(
                    args.fail_state_file,
                    network,
                    msg,
                    rows,
                    n_ok,
                    n_total,
                    aligned,
                )
                print(f"Fail state written to {args.fail_state_file}")
        except OSError as e:
            print(f"warning: could not write fail-state file: {e}", file=sys.stderr)

    return 0 if success else 1


if __name__ == "__main__":
    sys.exit(main())

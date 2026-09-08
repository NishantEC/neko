#!/usr/bin/env python3
"""Synthetic, local-only Codex app-server fixture for the evidence recipe.

It never reads a Codex session, network credential, project, or prompt.  Its
only task and approval values are the literal synthetic strings below.
"""

import argparse
import json
import os
import socket
import struct
import sys
import time


def log_method(method):
    """Record method names only; never write request parameters."""
    path = os.environ.get("NEKO_CODEX_FIXTURE_LOG")
    if path and method:
        with open(path, "a", encoding="utf-8") as log:
            log.write(f"{method}\n")


def write_message(message):
    sys.stdout.write(json.dumps(message, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def run_app_server():
    if os.environ.get("NEKO_CODEX_FIXTURE_STOP_FILE") and os.path.exists(
        os.environ["NEKO_CODEX_FIXTURE_STOP_FILE"]
    ):
        return 1

    pid_path = os.environ.get("NEKO_CODEX_FIXTURE_PID_FILE")
    if pid_path:
        with open(pid_path, "w", encoding="utf-8") as pid_file:
            pid_file.write(str(os.getpid()))

    for line in sys.stdin:
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        method = message.get("method")
        log_method(method)
        request_id = message.get("id")
        if method == "initialize":
            write_message(
                {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "result": {"capabilities": {"experimentalApi": True}},
                }
            )
        elif method == "thread/list":
            write_message(
                {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "result": {
                        "data": [
                            {
                                "id": "synthetic-task",
                                "name": "Synthetic task",
                                "status": "waiting",
                                "updatedAt": 0,
                            }
                        ]
                    },
                }
            )
            write_message(
                {
                    "jsonrpc": "2.0",
                    "id": 7001,
                    "method": "item/commandExecution/requestApproval",
                    "params": {
                        "threadId": "synthetic-task",
                        "command": "synthetic-command",
                        "reason": "Synthetic approval detail",
                    },
                }
            )
    return 0


def recv_exact(connection, length):
    chunks = []
    while length:
        chunk = connection.recv(length)
        if not chunk:
            raise RuntimeError("daemon socket closed while reading a frame")
        chunks.append(chunk)
        length -= len(chunk)
    return b"".join(chunks)


def probe(socket_path, expected_tasks, expected_approvals, expected_unavailable, min_apps):
    deadline = time.monotonic() + 10
    while True:
        connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        try:
            connection.connect(socket_path)
            break
        except OSError:
            connection.close()
            if time.monotonic() >= deadline:
                raise
            time.sleep(0.05)

    with connection:
        request = {
            "Request": {
                "id": 91,
                "request": {
                    "Search": {"query": "", "limit": 20, "provider": None}
                },
            }
        }
        payload = json.dumps(request, separators=(",", ":")).encode("utf-8")
        connection.sendall(struct.pack("<I", len(payload)) + payload)

        items = None
        while items is None:
            length = struct.unpack("<I", recv_exact(connection, 4))[0]
            frame = json.loads(recv_exact(connection, length))
            response = frame.get("Response", {}).get("response", {})
            result = response.get("SearchResults")
            if result and result.get("complete"):
                items = result["items"]

    tasks = [item for item in items if item.get("kind") == "codex-task"]
    approvals = [item for item in items if item.get("kind") == "codex-approval"]
    unavailable = [
        item
        for item in tasks
        if item.get("accessory") == "Codex unavailable"
        or "Codex unavailable" in (item.get("subtitle") or "")
    ]
    apps = [item for item in items if item.get("kind") == "app"]
    actual = (len(tasks), len(approvals), len(unavailable), len(apps))
    expected = (expected_tasks, expected_approvals, expected_unavailable)
    if actual[:3] != expected or actual[3] < min_apps:
        raise SystemExit(
            "unexpected sanitized counts "
            f"tasks={actual[0]} approvals={actual[1]} unavailable={actual[2]} apps={actual[3]}"
        )
    print(
        f"tasks={actual[0]} approvals={actual[1]} unavailable={actual[2]} apps={actual[3]}"
    )


def main():
    if sys.argv[1:] == ["app-server", "--stdio"]:
        return run_app_server()

    parser = argparse.ArgumentParser()
    parser.add_argument("--probe", metavar="SOCKET")
    parser.add_argument("--tasks", type=int, required=True)
    parser.add_argument("--approvals", type=int, required=True)
    parser.add_argument("--unavailable", type=int, required=True)
    parser.add_argument("--min-apps", type=int, default=1)
    args = parser.parse_args()
    if not args.probe:
        parser.error("--probe is required outside app-server mode")
    probe(args.probe, args.tasks, args.approvals, args.unavailable, args.min_apps)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

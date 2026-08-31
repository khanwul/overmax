#!/usr/bin/env python3
"""
Overmax IPC Client Reference Demo & Interactive CLI Dashboard.

This script demonstrates how to connect to the Overmax IPC Server using standard Python
(zero third-party dependencies required):
  1. Auto-discover host and port from `cache/ipc_endpoint.json`.
  2. Consume real-time game events over SSE (`GET /events`).
  3. Query game state and control overlay using JSON-RPC 2.0 (`POST /rpc`).

Usage:
  python examples/ipc_client_demo.py
"""

import json
import os
import sys
import threading
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any, Callable, Dict, Generator, List, Optional, Tuple

# ─────────────────────────────────────────────────────────────────────────────
# ANSI Color Formatting Utilities
# ─────────────────────────────────────────────────────────────────────────────

class Color:
    RESET = "\033[0m"
    BOLD = "\033[1m"
    DIM = "\033[2m"
    CYAN = "\033[96m"
    BLUE = "\033[94m"
    GREEN = "\033[92m"
    YELLOW = "\033[93m"
    RED = "\033[91m"
    MAGENTA = "\033[95m"
    GRAY = "\033[90m"


def log_event(name: str, msg: str, color: str = Color.CYAN):
    ts = time.strftime("%H:%M:%S")
    print(f"{Color.GRAY}[{ts}]{Color.RESET} {color}{Color.BOLD}[{name:^15}]{Color.RESET} {msg}")


# ─────────────────────────────────────────────────────────────────────────────
# Overmax IPC Client Class (Pure Python stdlib)
# ─────────────────────────────────────────────────────────────────────────────

class OvermaxIpcClient:
    """Lightweight Overmax IPC Client for SSE streaming and JSON-RPC 2.0."""

    DEFAULT_PORT = 30110
    PROTOCOL_ID = "overmax-ipc/1"

    def __init__(self, endpoint_file: Optional[Path] = None, fallback_port: int = DEFAULT_PORT):
        self.endpoint_file = endpoint_file or self._find_endpoint_file()
        self.fallback_port = fallback_port
        self.host = "127.0.0.1"
        self.port = fallback_port
        self._rpc_id = 1

    def _find_endpoint_file(self) -> Path:
        # Search current directory, parent directory, and project root
        candidates = [
            Path("cache/ipc_endpoint.json"),
            Path("../cache/ipc_endpoint.json"),
            Path(__file__).resolve().parent.parent / "cache" / "ipc_endpoint.json",
        ]
        for c in candidates:
            if c.exists():
                return c
        return candidates[0]

    def refresh_endpoint(self) -> bool:
        """Reads host and port from cache/ipc_endpoint.json if present."""
        if self.endpoint_file and self.endpoint_file.exists():
            try:
                data = json.loads(self.endpoint_file.read_text(encoding="utf-8"))
                if data.get("host") and data.get("port"):
                    self.host = data["host"]
                    self.port = int(data["port"])
                    return True
            except Exception:
                pass
        self.host = "127.0.0.1"
        self.port = self.fallback_port
        return False

    @property
    def base_url(self) -> str:
        return f"http://{self.host}:{self.port}"

    # ─────────────────────────────────────────────────────────────────────────
    # JSON-RPC 2.0 Queries & Commands
    # ─────────────────────────────────────────────────────────────────────────

    def rpc_call(self, method: str, params: Optional[List[Any]] = None) -> Tuple[bool, Any]:
        """Executes a JSON-RPC 2.0 call against /rpc endpoint."""
        url = f"{self.base_url}/rpc"
        payload = {
            "jsonrpc": "2.0",
            "id": self._rpc_id,
            "method": method,
            "params": params or [],
        }
        self._rpc_id += 1

        req = urllib.request.Request(
            url,
            data=json.dumps(payload).encode("utf-8"),
            headers={"Content-Type": "application/json"},
            method="POST",
        )

        try:
            with urllib.request.urlopen(req, timeout=3.0) as resp:
                result = json.loads(resp.read().decode("utf-8"))
                if "error" in result:
                    return False, result["error"]
                return True, result.get("result")
        except urllib.error.URLError as e:
            return False, str(e)
        except Exception as e:
            return False, str(e)

    def list_methods(self) -> Tuple[bool, Any]:
        return self.rpc_call("list_methods")

    def get_current_context(self) -> Tuple[bool, Any]:
        return self.rpc_call("get_current_context")

    def get_recommendations(self) -> Tuple[bool, Any]:
        return self.rpc_call("get_recommendations")

    def get_song_info(self, song_id: int) -> Tuple[bool, Any]:
        return self.rpc_call("get_song_info", [song_id])

    def get_recent_plays(self, mode: Optional[str] = None, limit: int = 10) -> Tuple[bool, Any]:
        params = [mode, limit] if mode else [None, limit]
        return self.rpc_call("get_recent_plays", params)

    def set_overlay_visibility(self, visible: bool) -> Tuple[bool, Any]:
        return self.rpc_call("set_overlay_visibility", [visible])

    # ─────────────────────────────────────────────────────────────────────────
    # SSE Stream Consumer (GET /events)
    # ─────────────────────────────────────────────────────────────────────────

    def stream_events(self) -> Generator[Dict[str, Any], None, None]:
        """Connects to /events and yields parsed SSE event objects indefinitely."""
        while True:
            self.refresh_endpoint()
            url = f"{self.base_url}/events"
            req = urllib.request.Request(url, headers={"Accept": "text/event-stream"})

            try:
                with urllib.request.urlopen(req, timeout=20.0) as resp:
                    log_event("CONNECTION", f"Connected to SSE stream at {self.base_url}/events", Color.GREEN)
                    current_event_type = "message"
                    buffer = ""

                    for line_bytes in resp:
                        line = line_bytes.decode("utf-8", errors="replace")
                        line_str = line.strip("\r\n")

                        if not line_str:
                            # Empty line signals end of SSE frame
                            if buffer:
                                try:
                                    event_data = json.loads(buffer)
                                    yield event_data
                                except json.JSONDecodeError:
                                    pass
                                buffer = ""
                                current_event_type = "message"
                            continue

                        if line_str.startswith(":"):
                            # SSE Comment / Heartbeat Ping (": ping")
                            continue

                        if line_str.startswith("event:"):
                            current_event_type = line_str[6:].strip()
                        elif line_str.startswith("data:"):
                            buffer += line_str[5:].strip()

            except (urllib.error.URLError, TimeoutError, ConnectionResetError, OSError) as e:
                log_event("DISCONNECTED", f"Connection lost ({e}). Retrying in 2 seconds...", Color.YELLOW)
                time.sleep(2.0)


# ─────────────────────────────────────────────────────────────────────────────
# Interactive CLI Dashboard Application
# ─────────────────────────────────────────────────────────────────────────────

def print_header(client: OvermaxIpcClient):
    print(f"\n{Color.CYAN}{Color.BOLD}╔═══════════════════════════════════════════════════════════════════════╗{Color.RESET}")
    print(f"{Color.CYAN}{Color.BOLD}║                   Overmax IPC Client Reference Demo                   ║{Color.RESET}")
    print(f"{Color.CYAN}{Color.BOLD}╠═══════════════════════════════════════════════════════════════════════╣{Color.RESET}")
    print(f"{Color.CYAN}{Color.BOLD}║{Color.RESET} Endpoint: {Color.GREEN}{client.base_url:<20}{Color.RESET} Protocol: {Color.YELLOW}{OvermaxIpcClient.PROTOCOL_ID:<16}{Color.RESET} {Color.CYAN}{Color.BOLD}║{Color.RESET}")
    print(f"{Color.CYAN}{Color.BOLD}╚═══════════════════════════════════════════════════════════════════════╝{Color.RESET}\n")


def sse_worker_thread(client: OvermaxIpcClient, is_running: threading.Event):
    """Background listener for SSE events with rich formatting and auto RPC lookup."""
    for event in client.stream_events():
        if not is_running.is_set():
            break

        event_type = event.get("type", "unknown")
        seq = event.get("seq", 0)
        payload = event.get("payload", {})

        if event_type == "state_snapshot":
            scene = payload.get("scene", "Unknown")
            stable = payload.get("stable", False)
            ctx = payload.get("context") or {}
            song_id = ctx.get("song_id")
            mode = ctx.get("mode", "")
            diff = ctx.get("diff", "")
            rate = ctx.get("rate")
            rate_str = f"{rate:.2f}%" if rate is not None else "Unplayed"
            log_event(
                "STATE_SNAPSHOT",
                f"Scene: {Color.BOLD}{scene}{Color.RESET} | Stable: {stable} | Song: #{song_id} [{mode} {diff}] ({rate_str})",
                Color.MAGENTA,
            )

        elif event_type == "scene_detected":
            scene = payload.get("scene", "Unknown")
            log_event("SCENE", f"Entered scene: {Color.BOLD}{scene}{Color.RESET}", Color.CYAN)

        elif event_type == "song_detected":
            song_id = payload.get("song_id", 0)
            mode = payload.get("mode", "")
            diff = payload.get("diff", "")
            rate = payload.get("rate", 0.0)
            title = payload.get("title") or f"Song #{song_id}"
            floor_name = payload.get("floor_name") or "N/A"
            rate_str = f"{rate:.2f}%" if rate > 0.0 else "Unplayed"

            log_event(
                "SONG_DETECTED",
                f"Track: {Color.BOLD}{title}{Color.RESET} [{mode} {diff} | {Color.YELLOW}{floor_name}{Color.RESET}] — Rate: {rate_str}",
                Color.BLUE,
            )

            # Proactive Demonstration: Query full song metadata via JSON-RPC
            ok, info = client.get_song_info(song_id)
            if ok and isinstance(info, dict):
                composer = info.get("composer", "Unknown")
                dlc = info.get("dlc", "RESPECT")
                patterns = info.get("patterns", [])
                p_summary = ", ".join([f"{p.get('mode')} {p.get('diff')}: Lv.{p.get('level')}" for p in patterns[:4]])
                log_event("RPC ➔ SONG_INFO", f"Composer: {composer} | DLC: {dlc} | Patterns: {p_summary}...", Color.GRAY)

        elif event_type == "play_verified":
            song_id = payload.get("song_id", 0)
            mode = payload.get("mode", "")
            diff = payload.get("diff", "")
            rate = payload.get("rate", 0.0)
            is_max = payload.get("is_max_combo", False)
            is_pb = payload.get("is_pb", False)
            title = payload.get("title") or f"Song #{song_id}"

            combo_str = f"{Color.GREEN}[MAX COMBO]{Color.RESET}" if is_max else ""
            pb_str = f"{Color.YELLOW}[NEW PB!]{Color.RESET}" if is_pb else ""

            log_event(
                "PLAY_VERIFIED",
                f"🎉 {Color.BOLD}{title}{Color.RESET} [{mode} {diff}] Score: {Color.GREEN}{rate:.2f}%{Color.RESET} {combo_str} {pb_str}",
                Color.GREEN,
            )


def print_menu():
    print(f"\n{Color.BOLD}--- Interactive Commands ---{Color.RESET}")
    print(f" [{Color.CYAN}1{Color.RESET}] Query Current Session Context   (`get_current_context`)")
    print(f" [{Color.CYAN}2{Color.RESET}] Query Real-time Recommendations (`get_recommendations`)")
    print(f" [{Color.CYAN}3{Color.RESET}] Query Recent Play History       (`get_recent_plays`)")
    print(f" [{Color.CYAN}4{Color.RESET}] Toggle In-Game Overlay          (`set_overlay_visibility`)")
    print(f" [{Color.CYAN}5{Color.RESET}] List All RPC Methods            (`list_methods`)")
    print(f" [{Color.RED}q{Color.RESET}] Quit Demo\n")


def main():
    client = OvermaxIpcClient()
    client.refresh_endpoint()
    print_header(client)

    is_running = threading.Event()
    is_running.set()

    # Launch background SSE listener
    sse_thread = threading.Thread(target=sse_worker_thread, args=(client, is_running), daemon=True)
    sse_thread.start()

    overlay_visible = True

    try:
        while is_running.is_set():
            time.sleep(0.5)
            print_menu()
            choice = input(f"{Color.BOLD}Select command (1-5, q): {Color.RESET}").strip()

            if choice == "1":
                ok, ctx = client.get_current_context()
                if ok:
                    print(f"\n{Color.GREEN}=== Current Context ==={Color.RESET}")
                    print(json.dumps(ctx, indent=2, ensure_ascii=False))
                else:
                    print(f"{Color.RED}RPC Error: {ctx}{Color.RESET}")

            elif choice == "2":
                ok, recs = client.get_recommendations()
                if ok:
                    print(f"\n{Color.GREEN}=== Real-Time Recommendations ==={Color.RESET}")
                    print(json.dumps(recs, indent=2, ensure_ascii=False))
                else:
                    print(f"{Color.RED}RPC Error: {recs}{Color.RESET}")

            elif choice == "3":
                ok, plays = client.get_recent_plays(limit=5)
                if ok:
                    print(f"\n{Color.GREEN}=== Recent Plays (Last 5) ==={Color.RESET}")
                    print(json.dumps(plays, indent=2, ensure_ascii=False))
                else:
                    print(f"{Color.RED}RPC Error: {plays}{Color.RESET}")

            elif choice == "4":
                overlay_visible = not overlay_visible
                ok, _ = client.set_overlay_visibility(overlay_visible)
                status = "VISIBLE" if overlay_visible else "HIDDEN"
                if ok:
                    log_event("RPC ➔ OVERLAY", f"Set overlay visibility: {Color.BOLD}{status}{Color.RESET}", Color.YELLOW)
                else:
                    print(f"{Color.RED}RPC Error setting visibility{Color.RESET}")

            elif choice == "5":
                ok, methods = client.list_methods()
                if ok:
                    print(f"\n{Color.GREEN}=== Available Methods ==={Color.RESET}")
                    print(json.dumps(methods, indent=2))
                else:
                    print(f"{Color.RED}RPC Error: {methods}{Color.RESET}")

            elif choice.lower() == "q":
                break

    except KeyboardInterrupt:
        pass
    finally:
        is_running.clear()
        print(f"\n{Color.GRAY}Shutting down demo...{Color.RESET}")


if __name__ == "__main__":
    main()

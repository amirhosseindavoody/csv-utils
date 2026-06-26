#!/usr/bin/env python3
"""
Capture a terminal snapshot of the TUI as rendered in a PTY.

This tool runs the command inside a pseudo terminal, collects raw terminal bytes,
applies a minimal ANSI/CSI parser to reconstruct the visible screen, and writes:
  1) a human-readable snapshot report
  2) the raw byte stream (for low-level debugging)
"""

from __future__ import annotations

import argparse
import collections
import os
import shlex
import subprocess
from typing import List


class AnsiScreen:
    def __init__(self, rows: int, cols: int) -> None:
        self.rows = rows
        self.cols = cols
        self.buf: List[List[str]] = [[" "] * cols for _ in range(rows)]
        self.r = 0
        self.c = 0
        self.state = "normal"
        self.csi = ""
        self.osc = ""
        self.saved = (0, 0)

    def feed(self, data: bytes) -> None:
        for b in data:
            self._byte(b)

    def _byte(self, b: int) -> None:
        if self.state == "normal":
            if b == 0x1B:
                self.state = "esc"
            elif b == 0x0D:  # CR
                self.c = 0
            elif b == 0x0A:  # LF
                self.r = min(self.rows - 1, self.r + 1)
            elif b == 0x08:  # BS
                self.c = max(0, self.c - 1)
            elif b == 0x09:  # TAB
                self.c = min(self.cols - 1, ((self.c // 8) + 1) * 8)
            elif 32 <= b <= 126:
                self._put(chr(b))
            elif b >= 160:
                # Treat high bytes as replacement markers for visibility.
                self._put("?")
            return

        if self.state == "esc":
            if b == ord("["):
                self.state = "csi"
                self.csi = ""
            elif b == ord("]"):
                self.state = "osc"
                self.osc = ""
            elif b == ord("7"):
                self.saved = (self.r, self.c)
                self.state = "normal"
            elif b == ord("8"):
                self.r, self.c = self.saved
                self.state = "normal"
            elif b in (ord("D"), ord("E")):
                self.r = min(self.rows - 1, self.r + 1)
                if b == ord("E"):
                    self.c = 0
                self.state = "normal"
            elif b == ord("M"):
                self.r = max(0, self.r - 1)
                self.state = "normal"
            elif b == ord("c"):
                self._clear()
                self.state = "normal"
            else:
                self.state = "normal"
            return

        if self.state == "osc":
            # OSC terminated by BEL or ST (ESC \)
            if b == 0x07:
                self.state = "normal"
            elif b == 0x1B:
                self.state = "osc_esc"
            else:
                self.osc += chr(b)
            return

        if self.state == "osc_esc":
            self.state = "normal"
            return

        if self.state == "csi":
            if 0x40 <= b <= 0x7E:
                self._apply_csi(self.csi, chr(b))
                self.state = "normal"
            else:
                self.csi += chr(b)

    def _put(self, ch: str) -> None:
        if 0 <= self.r < self.rows and 0 <= self.c < self.cols:
            self.buf[self.r][self.c] = ch
        self.c += 1
        if self.c >= self.cols:
            self.c = self.cols - 1

    def _clear(self) -> None:
        self.buf = [[" "] * self.cols for _ in range(self.rows)]
        self.r = 0
        self.c = 0

    def _erase_line_from_cursor(self) -> None:
        if 0 <= self.r < self.rows:
            for i in range(self.c, self.cols):
                self.buf[self.r][i] = " "

    def _apply_csi(self, params: str, final: str) -> None:
        parts = [p for p in params.split(";") if p] if params else []
        nums = [int(p) if p.isdigit() else 0 for p in parts]

        def n(i: int, default: int) -> int:
            return nums[i] if i < len(nums) else default

        if final in ("H", "f"):
            rr = max(1, n(0, 1)) - 1
            cc = max(1, n(1, 1)) - 1
            self.r = min(self.rows - 1, rr)
            self.c = min(self.cols - 1, cc)
        elif final == "A":
            self.r = max(0, self.r - n(0, 1))
        elif final == "B":
            self.r = min(self.rows - 1, self.r + n(0, 1))
        elif final == "C":
            self.c = min(self.cols - 1, self.c + n(0, 1))
        elif final == "D":
            self.c = max(0, self.c - n(0, 1))
        elif final == "J":
            mode = n(0, 0)
            if mode in (2, 3):
                self._clear()
        elif final == "K":
            self._erase_line_from_cursor()
        elif final == "s":
            self.saved = (self.r, self.c)
        elif final == "u":
            self.r, self.c = self.saved
        # Ignore SGR ('m') and others for plain-text snapshot.

    def snapshot(self) -> str:
        return "\n".join("".join(row).rstrip() for row in self.buf)


def main() -> int:
    parser = argparse.ArgumentParser(description="Capture a TUI snapshot from PTY output.")
    parser.add_argument("--file", required=True, help="CSV file path for `pixi run csv`.")
    parser.add_argument("--seconds", type=float, default=2.0, help="How long to capture before sending q.")
    parser.add_argument("--rows", type=int, default=40, help="PTY rows.")
    parser.add_argument("--cols", type=int, default=140, help="PTY cols.")
    parser.add_argument(
        "--command-template",
        default='./target/release/csv "{file}"',
        help='Command template to run inside PTY. Use "{file}" placeholder.',
    )
    parser.add_argument("--output", default="artifacts/tui_snapshot_large.txt", help="Snapshot report output path.")
    parser.add_argument("--raw-output", default="artifacts/tui_snapshot_large.raw", help="Raw byte output path.")
    args = parser.parse_args()

    os.makedirs(os.path.dirname(args.output), exist_ok=True)
    os.makedirs(os.path.dirname(args.raw_output), exist_ok=True)

    command = args.command_template.format(file=args.file)
    env = os.environ.copy()
    env["TERM"] = env.get("TERM", "xterm-256color")
    env["COLUMNS"] = str(args.cols)
    env["LINES"] = str(args.rows)

    # Use script(1) for robust PTY ownership so /dev/tty opens correctly.
    inner = f"stty rows {args.rows} cols {args.cols}; {command}"
    wrapped = (
        f"(sleep {args.seconds}; printf q) | "
        f"script -qefc {shlex.quote(inner)} /dev/null"
    )
    proc = subprocess.run(
        ["sh", "-lc", wrapped],
        check=False,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    raw = proc.stdout or b""

    with open(args.raw_output, "wb") as f:
        f.write(raw)

    screen = AnsiScreen(args.rows, args.cols)
    screen.feed(bytes(raw))
    snap = screen.snapshot()

    bad = [b for b in raw if not (b in (9, 10, 13, 27) or 32 <= b <= 126)]
    freq = collections.Counter(bad).most_common(20)

    with open(args.output, "w", encoding="utf-8") as f:
        f.write(f"command: {command}\n")
        f.write(f"pty: {args.rows}x{args.cols}\n")
        f.write(f"bytes_captured: {len(raw)}\n")
        f.write(f"non_ascii_or_control_bytes: {len(bad)}\n")
        f.write(f"raw_output: {args.raw_output}\n")
        f.write("\nnon_ascii_top20:\n")
        for b, c in freq:
            f.write(f"  0x{b:02x}: {c}\n")
        f.write("\n=== snapshot ===\n")
        f.write(snap)
        f.write("\n")

    print(f"Wrote snapshot report: {args.output}")
    print(f"Wrote raw terminal bytes: {args.raw_output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

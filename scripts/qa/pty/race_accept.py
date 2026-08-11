"""Reproduce the stale-result acceptance race in a real pty.

Scenario: 400k candidates. Type `ab`, wait for the search to land, then send the
next keystroke and Enter together so Enter is processed before the requeued
search for the new query completes. What the binary prints on its stdout is the
accepted line.

usage: race_accept.py <binary> <corpus> <settled-query> <extra-keys> [--label L]
"""

import fcntl
import os
import select
import struct
import sys
import termios
import time


def set_size(fd, rows, cols):
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))


def drain(master, timeout):
    out = b""
    deadline = time.time() + timeout
    while True:
        remaining = deadline - time.time()
        if remaining <= 0:
            return out
        ready, _, _ = select.select([master], [], [], remaining)
        if not ready:
            continue
        try:
            chunk = os.read(master, 1 << 16)
        except OSError:
            return out
        if not chunk:
            return out
        out += chunk


def run(binary, corpus, settled, extra, settle=4.0, args=()):
    cand_r, cand_w = os.pipe()
    out_r, out_w = os.pipe()
    pid, master = os.forkpty()
    if pid == 0:
        os.dup2(cand_r, 0)
        os.dup2(out_w, 1)  # accepted line goes here; TUI paints on stderr==pty
        for fd in (cand_w, out_r):
            os.close(fd)
        os.environ["YURU_CONFIG_FILE"] = "/nonexistent"
        os.environ["TERM"] = "xterm-256color"
        os.execv(binary, [binary, *args])
        os._exit(127)

    os.close(cand_r)
    os.close(out_w)
    set_size(master, 24, 80)

    data = open(corpus, "rb").read()
    written = 0
    while written < len(data):
        written += os.write(cand_w, data[written : written + (1 << 16)])
        drain(master, 0.0)
    os.close(cand_w)

    drain(master, 1.0)
    os.write(master, settled.encode())
    frame = drain(master, settle)  # let the settled query's search finish
    # Race: new keystroke(s) + Enter arrive in one burst.
    os.write(master, extra.encode() + b"\r")
    race_started = time.time()
    accepted = b""
    deadline = time.time() + 20.0
    while time.time() < deadline:
        ready, _, _ = select.select([out_r, master], [], [], 0.2)
        if out_r in ready:
            chunk = os.read(out_r, 1 << 16)
            if not chunk:
                break
            accepted += chunk
        if master in ready:
            try:
                if not os.read(master, 1 << 16):
                    break
            except OSError:
                break
        if accepted.endswith(b"\n"):
            break
    _, status = os.waitpid(pid, 0)
    os.close(out_r)
    try:
        os.close(master)
    except OSError:
        pass
    elapsed = time.time() - race_started
    return (
        accepted.decode("utf-8", "replace"),
        os.waitstatus_to_exitcode(status),
        elapsed,
    )


def main():
    binary, corpus, settled, extra = sys.argv[1:5]
    settle = float(sys.argv[5]) if len(sys.argv) > 5 else 4.0
    label = sys.argv[6] if len(sys.argv) > 6 else binary
    args = sys.argv[7:]
    accepted, code, elapsed = run(binary, corpus, settled, extra, settle, args)
    live = settled + extra
    print(f"--- {label}")
    print(f"    corpus={os.path.basename(corpus)} typed={settled!r} then {extra!r}+Enter args={list(args)}")
    print(f"    live query = {live!r}")
    print(f"    exit code  = {code}")
    print(f"    accepted   = {accepted!r}")
    print(f"    Enter->exit = {elapsed * 1000:.0f} ms")


if __name__ == "__main__":
    main()

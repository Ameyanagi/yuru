"""Drive the yuru TUI in a pty: check live smart case and resize repaint."""

import fcntl
import os
import select
import struct
import sys
import termios
import time

BIN = os.environ.get("YURU_QA_BIN") or os.path.join(
    os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))),
    "target", "release", "yuru",
)
CANDIDATES = b"abc\nABC\nAxx\n"


def set_size(fd, rows, cols):
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))


def drain(master, timeout=0.8):
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
            chunk = os.read(master, 65536)
        except OSError:
            return out
        if not chunk:
            return out
        out += chunk


def main():
    stdin_r, stdin_w = os.pipe()
    pid, master = os.forkpty()
    if pid == 0:
        os.dup2(stdin_r, 0)
        os.close(stdin_w)
        os.environ["YURU_CONFIG_FILE"] = "/nonexistent"
        os.execv(BIN, [BIN] + sys.argv[1:])
        os._exit(127)

    os.close(stdin_r)
    set_size(master, 24, 80)
    os.write(stdin_w, CANDIDATES)
    os.close(stdin_w)

    frames = {}
    frames["initial"] = drain(master)
    os.write(master, b"A")
    frames["after_upper_A"] = drain(master)
    os.write(master, b"\x7f")
    frames["after_backspace"] = drain(master)
    os.write(master, b"a")
    frames["after_lower_a"] = drain(master)
    set_size(master, 12, 40)
    frames["after_resize"] = drain(master)
    os.write(master, b"\x1b")
    drain(master, 0.4)
    os.waitpid(pid, 0)

    for name, data in frames.items():
        print(f"=== {name} ({len(data)} bytes)")
        print(repr(data.decode("utf-8", "replace")))
        print()


main()

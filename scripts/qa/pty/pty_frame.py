"""Run yuru interactively in a pty of a fixed size and dump the first frame.

usage: pty_frame.py <binary> <cols> <rows> <candidate-file> [args...]
"""

import fcntl
import os
import select
import signal
import struct
import sys
import termios
import time

binary, cols, rows, cands, *args = sys.argv[1:]
cols, rows = int(cols), int(rows)

master, slave = os.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))

pid = os.fork()
if pid == 0:
    os.setsid()
    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    os.dup2(slave, 1)
    os.dup2(slave, 2)
    fd = os.open(cands, os.O_RDONLY)
    os.dup2(fd, 0)
    os.close(master)
    os.environ["TERM"] = "xterm-256color"
    os.execv(binary, [binary, *args])

os.close(slave)
out = b""
deadline = time.time() + 2.0
while time.time() < deadline:
    ready, _, _ = select.select([master], [], [], 0.2)
    if not ready:
        continue
    try:
        chunk = os.read(master, 65536)
    except OSError:
        break
    if not chunk:
        break
    out += chunk

os.kill(pid, signal.SIGKILL)
os.waitpid(pid, 0)
sys.stdout.write(repr(out.decode("utf-8", "replace")) + "\n")

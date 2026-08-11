#!/usr/bin/env python3
"""Deterministically generate the ASCII and CJK benchmark corpora."""

import os
import random
import string

HERE = os.path.dirname(os.path.abspath(__file__))

import common  # noqa: E402


def gen_ascii(path: str) -> None:
    random.seed(1)
    words = [
        "".join(random.choices(string.ascii_lowercase, k=random.randint(3, 9)))
        for _ in range(4000)
    ]
    with open(path, "w", encoding="utf-8") as fh:
        for i in range(500000):
            fh.write("/".join(random.choices(words, k=3)) + "/file%d.rs\n" % i)


def gen_cjk(path: str) -> None:
    random.seed(7)
    pieces = [
        "日本語",
        "検索",
        "東京駅",
        "カメラ",
        "写真",
        "北京大学",
        "한글",
        "候補",
        "資料",
        "会議",
        "報告書",
        "画像",
    ]
    with open(path, "w", encoding="utf-8") as fh:
        for i in range(50000):
            fh.write("/".join(random.choices(pieces, k=3)) + "/file%d.txt\n" % i)


def gen_mixed_case(path: str) -> None:
    """Mixed-case corpus. Exercises case folding, word boundaries and camelCase -
    the blind spot that let a default-path ranking change through unnoticed."""
    random.seed(42)
    words = [
        "README", "readme", "ReadMe", "SrcMain", "src_main", "src-main",
        "MyFile", "myfile", "HTTPServer", "httpserver", "Http_Server",
        "TestCase", "testcase", "test_case", "AbcDef", "abcdef", "ABC_DEF",
        "Foo", "foo", "FOO", "BarBaz", "barbaz", "bar-baz",
    ]
    exts = ["rs", "md", "TXT", "Txt", "py"]
    dirs = ["Src", "src", "SRC", "Docs", "docs", "lib", "Lib"]
    with open(path, "w", encoding="utf-8") as fh:
        for i in range(30000):
            fh.write(
                "/".join(random.choices(dirs, k=random.randint(1, 3)))
                + "/"
                + random.choice(words)
                + str(random.randint(0, 99))
                + "."
                + random.choice(exts)
                + "\n"
            )


def gen_tiny(path: str) -> None:
    """Four spellings of one name. The exact-case tie-break lives or dies here."""
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("readme.md\nREADME.md\nReadMe.md\nreadme_old.md\n")


def gen_race(path: str) -> None:
    """Stale-result race corpus. `ab` matches all three heads; `abC` matches only
    the last two, so a stale selection index maps to the wrong row. The long tail
    makes the replacement search slow enough to lose the race."""
    z = "z" * 50
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("ABC\n")
        fh.write(z + "abC-one\n")
        fh.write(z + "abC-two\n")
        for i in range(100000):
            fh.write("q" * 60 + str(i) + "\n")


if __name__ == "__main__":
    work = common.ensure_work()
    gen_ascii(common.BIG)
    gen_cjk(common.CJK)
    gen_mixed_case(common.CASE)
    gen_tiny(common.TINY)
    gen_race(common.RACE)
    print(f"corpora written to {work}")
    for path in (common.BIG, common.CJK, common.CASE, common.TINY, common.RACE):
        with open(path, encoding="utf-8") as fh:
            print(f"  {os.path.basename(path):12s} {sum(1 for _ in fh):>7d} lines")

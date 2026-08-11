mod support;

use predicates::prelude::*;
use support::command;

#[test]
fn cli_ignore_case_matches_mixed_case_candidate() {
    command()
        .args(["--filter", "abc", "--ignore-case"])
        .write_stdin("ABC\n")
        .assert()
        .success()
        .stdout(predicate::eq("ABC\n"));
}

#[test]
fn cli_ignore_case_survives_literal() {
    command()
        .args(["--filter", "abc", "--ignore-case", "--literal"])
        .write_stdin("ABC\n")
        .assert()
        .success()
        .stdout(predicate::eq("ABC\n"));
}

#[test]
fn cli_ignore_case_survives_literal_for_uppercase_query() {
    command()
        .args(["--filter", "ABC", "--ignore-case", "--literal"])
        .write_stdin("abc\n")
        .assert()
        .success()
        .stdout(predicate::eq("abc\n"));
}

#[test]
fn cli_no_ignore_case_stays_case_sensitive() {
    command()
        .args(["--filter", "abc", "--no-ignore-case"])
        .write_stdin("ABC\nabc\n")
        .assert()
        .success()
        .stdout(predicate::eq("abc\n"));
}

#[test]
fn cli_no_ignore_case_stays_case_sensitive_with_literal() {
    command()
        .args(["--filter", "abc", "--no-ignore-case", "--literal"])
        .write_stdin("ABC\nabc\n")
        .assert()
        .success()
        .stdout(predicate::eq("abc\n"));
}

#[test]
fn cli_smart_case_treats_uppercase_query_as_case_sensitive() {
    command()
        .args(["--filter", "ABC"])
        .write_stdin("ABC\nabc\n")
        .assert()
        .success()
        .stdout(predicate::eq("ABC\n"));
}

#[test]
fn cli_smart_case_treats_lowercase_query_as_case_insensitive() {
    command()
        .args(["--filter", "abc"])
        .write_stdin("ABC\nxyz\n")
        .assert()
        .success()
        .stdout(predicate::eq("ABC\n"));
}

#[test]
fn cli_exact_mode_ignores_case_by_default() {
    command()
        .args(["--filter", "abc", "--exact"])
        .write_stdin("xxABCxx\nnope\n")
        .assert()
        .success()
        .stdout(predicate::eq("xxABCxx\n"));
}

#[test]
fn cli_exact_mode_ignores_case_with_literal() {
    command()
        .args(["--filter", "abc", "--exact", "--ignore-case", "--literal"])
        .write_stdin("xxABCxx\nnope\n")
        .assert()
        .success()
        .stdout(predicate::eq("xxABCxx\n"));
}

#[test]
fn cli_exact_mode_honours_no_ignore_case() {
    command()
        .args(["--filter", "abc", "--exact", "--no-ignore-case"])
        .write_stdin("xxABCxx\nxxabcxx\n")
        .assert()
        .success()
        .stdout(predicate::eq("xxabcxx\n"));
}

#[test]
fn cli_extended_terms_ignore_case_with_literal() {
    command()
        .args(["--filter", "'abc !xyz", "--ignore-case", "--literal"])
        .write_stdin("xxABCxx\nABC-XYZ\n")
        .assert()
        .success()
        .stdout(predicate::eq("xxABCxx\n"));
}

#[test]
fn cli_ignore_case_survives_literal_for_non_ascii_text() {
    command()
        .args(["--filter", "éa", "--ignore-case", "--literal"])
        .write_stdin("ÉA.txt\n")
        .assert()
        .success()
        .stdout(predicate::eq("ÉA.txt\n"));
}

/// An extended exact term scored `FOO` and `foo` the same, so the winner was whichever the
/// input listed first. Both orders must now put the literal spelling first.
#[test]
fn cli_extended_exact_term_ranks_the_literal_spelling_first() {
    for stdin in ["FOO\nfoo\n", "foo\nFOO\n"] {
        command()
            .args(["--filter", "'foo", "--ignore-case"])
            .write_stdin(stdin)
            .assert()
            .success()
            .stdout(predicate::eq("foo\nFOO\n"));
    }
}

/// `'readme` tied `README.md` with `readme.md` at 9991 apiece while single-term `--exact
/// readme` ranked the literal spelling first. `--tiebreak=index` makes the tie observable:
/// with one, the answer follows the input order instead of the query's spelling.
#[test]
fn cli_extended_exact_term_orders_case_variants_like_the_global_exact_path() {
    for stdin in ["README.md\nreadme.md\n", "readme.md\nREADME.md\n"] {
        for args in [
            ["--filter", "'readme"].as_slice(),
            ["--filter", "readme", "--exact"].as_slice(),
        ] {
            command()
                .args(args)
                .args(["--ignore-case", "--tiebreak=index", "--limit", "1"])
                .write_stdin(stdin)
                .assert()
                .success()
                .stdout(predicate::eq("readme.md\n"));
        }
    }
}

/// `İ` lowercases to `i` plus a combining dot above, so `İa` contains no plain `i` and an
/// exact `ia` must not match it. Folding kept only the `i` and reported a false positive.
#[test]
fn cli_exact_ignore_case_does_not_match_across_a_combining_lowercase_tail() {
    command()
        .args([
            "--filter",
            "ia",
            "--ignore-case",
            "--exact",
            "--lang",
            "plain",
        ])
        .write_stdin("İa\n")
        .assert()
        .code(1)
        .stdout(predicate::eq(""));
}

/// The same guard under `--literal`, where nothing but the matcher's own folding is left to
/// enforce it.
#[test]
fn cli_exact_ignore_case_does_not_match_across_a_combining_lowercase_tail_with_literal() {
    command()
        .args([
            "--filter",
            "ia",
            "--ignore-case",
            "--exact",
            "--literal",
            "--lang",
            "plain",
        ])
        .write_stdin("İa\n")
        .assert()
        .code(1)
        .stdout(predicate::eq(""));
}

/// Keeping the combining tail must not cost the match to the tail: `İ` and `i` + U+0307 are
/// the same text case-insensitively, whichever side spells it as one character.
///
/// `--literal` is the case that has to be spelled out. With normalization on, the normalized
/// key carries the written-out lowercase form and covers this; with it off there is no such
/// key, and refusing to fold `İ` at all turned the earlier false positive into a false
/// negative.
#[test]
fn cli_ignore_case_survives_literal_for_a_multi_char_lowercase_mapping() {
    for (query, stdin) in [
        ("i\u{307}stanbul", "İstanbul.txt\n"),
        ("İstanbul", "i\u{307}stanbul.txt\n"),
    ] {
        for mode in [&["--exact"][..], &[]] {
            command()
                .args(["--filter", query, "--ignore-case", "--literal"])
                .args(mode)
                .args(["--lang", "plain"])
                .write_stdin(stdin)
                .assert()
                .success()
                .stdout(predicate::eq(stdin));
        }
    }
}

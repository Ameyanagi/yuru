mod support;

use predicates::prelude::*;
use support::command;

/// Runs a case-insensitive plain-language filter under `--explain` and returns each output
/// record with the score reported for it, in ranked order.
///
/// Order alone cannot tell a win from a tie the tiebreak resolved, which is what let a
/// ranking test pass with the exact-case bonus removed. The scores can.
fn explain_scores(args: &[&str], stdin: &str) -> Vec<(String, i64)> {
    let output = command()
        .args(["--ignore-case", "--lang", "plain", "--explain"])
        .args(args)
        .write_stdin(stdin.to_string())
        .output()
        .expect("yuru ran");
    assert!(output.status.success(), "yuru {args:?} failed");

    let stdout = String::from_utf8(output.stdout).expect("UTF-8 output");
    let mut ranked: Vec<(String, i64)> = Vec::new();
    for line in stdout.lines() {
        // `--explain` prints the record itself unindented and every detail of it indented.
        match line.strip_prefix("  score: ") {
            Some(score) => {
                ranked.last_mut().expect("a record before its score").1 =
                    score.parse().expect("an integer score");
            }
            None if !line.starts_with("  ") => ranked.push((line.to_string(), 0)),
            None => {}
        }
    }
    ranked
}

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

/// Every matcher path must rank the candidate spelled the way the query was typed first,
/// whichever of the two spellings of `İ` the query used.
///
/// `--literal` is where the matcher's own folding is all there is to enforce this. v0.2.0
/// scored the written-out copy of the query against the written-out copy of the candidate, so
/// `i` + U+0307 collected a second character's `SCORE_MATCH` and a consecutive bonus that the
/// one-character `İ` cannot collect, and won a query typed `İ` outright. With the query typed
/// the other way round the same inflation produced a tie that the length tiebreak resolved
/// towards `İ`, while an extended exact term - which never awarded the exact-case bonus
/// through an expansion - put `i` + U+0307 first. Four paths, three answers.
///
/// A query typed `İ` is only listed under `--literal` because with normalization on the
/// query's own normalized variant *is* `i` + U+0307, so the candidate spelled that way is
/// matched by a variant of the query as typed rather than through the matcher's rewrite. What
/// wins there is decided by the query-variant weights, not by this.
///
/// The two candidates are one and two characters long, so the order alone is weak evidence:
/// the length tiebreak would put `İ` first on its own, and for a query typed `İ` that is in
/// fact all that decides it. So this asserts the reported *scores*, and asserts per cell
/// whether the win is earned by score or conceded to the tiebreak.
#[test]
fn cli_ignore_case_ranks_the_spelling_the_query_was_typed_with_first() {
    let stdin = "i\u{307}\nİ\n";
    for (query, expected, literal) in [
        ("İ", "İ", &["--literal"][..]),
        ("i\u{307}", "i\u{307}", &["--literal"][..]),
        ("i\u{307}", "i\u{307}", &[][..]),
    ] {
        for (filter, algo, decided_by_score) in [
            (query.to_string(), &[][..], true),
            (query.to_string(), &["--exact"][..], true),
            (query.to_string(), &["--algo", "nucleo"][..], true),
            // An extended exact term typed `İ` is folded to `i` + U+0307 before it is looked
            // up, so it is not the 1:1 fold of itself and neither spelling can collect the
            // exact-case bonus. That one cell is a real tie, and the length tiebreak - not a
            // score - is what resolves it towards the spelling the query used.
            (format!("'{query}"), &["--extended"][..], query != "İ"),
        ] {
            let mut args = vec!["--filter", filter.as_str()];
            args.extend(literal);
            args.extend(algo);
            let ranked = explain_scores(&args, stdin);
            let context = format!("{filter:?} {literal:?} {algo:?}");

            assert_eq!(
                ranked.first().map(|(text, _)| text.as_str()),
                Some(expected)
            );
            // `--algo nucleo` folds `İ` only towards itself, so under `--literal` the other
            // spelling does not match at all and there is no runner-up to compare against.
            if let Some((runner_up, score)) = ranked.get(1) {
                if decided_by_score {
                    assert!(
                        ranked[0].1 > *score,
                        "{context}: {expected:?} must outscore {runner_up:?}, got {} vs {score}",
                        ranked[0].1
                    );
                } else {
                    assert_eq!(ranked[0].1, *score, "{context}: expected a scoring tie");
                }
            }
        }
    }
}

/// The exact-case bonus belongs to the occurrence that was matched, not to the candidate.
///
/// `İ` is the one character whose lowercase mapping is two characters, so a candidate holding
/// it does not fold to itself character for character. Requiring that of the whole candidate
/// made every term in the query pay for it - here a pure-ASCII term matching an `a` the `İ` is
/// nowhere near, which the same text spelled `i` + U+0307 collected the bonus for. Two
/// spellings of one line ranked 75 points apart on a term neither spelling touches.
///
/// The controls say the same thing from the other side: `U+212A` KELVIN SIGN folds to a
/// one-byte `k`, resizing the candidate in bytes without resizing it in characters, and it
/// scored - and still scores - exactly like plain ASCII.
#[test]
fn cli_ignore_case_exact_term_bonus_ignores_folding_elsewhere_in_the_candidate() {
    let ranked = explain_scores(&["--filter", "'a", "--literal"], "İ a\ni\u{307} a\nİ A\n");
    let score = |text: &str| {
        ranked
            .iter()
            .find(|(display, _)| display == text)
            .unwrap_or_else(|| panic!("{text:?} matched"))
            .1
    };

    assert_eq!(
        score("İ a"),
        score("i\u{307} a"),
        "the two spellings fold to the same text and the term matches neither of them"
    );
    assert!(
        score("İ a") > score("İ A"),
        "and both collect the bonus rather than both losing it"
    );

    let controls = explain_scores(&["--filter", "'a", "--literal"], "X a\n\u{212a} a\n");
    assert_eq!(controls[0].1, controls[1].1, "KELVIN SIGN folds 1:1");
}

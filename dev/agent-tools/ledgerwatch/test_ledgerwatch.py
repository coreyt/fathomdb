"""Tests for ledgerwatch.

Happy paths verify the delta is emitted; failure-mode tests verify the tool
does not *miss* an update (the expensive silent failure) for each file format.
"""

import io
import json
import os

import ledgerwatch


# Grep-style exit contract (the default). Tests pin the code AND the output so a
# no-change result can never be satisfied by a silently-broken run: errors are a
# distinct code, proven by the negative-control tests below.
CHANGED = 0
NO_CHANGE = 1
ERROR = 2


def lw(tmp_path, *args, no_status=False):
    """Invoke run() against a private state dir; return (rc, stdout, stderr)."""
    out, err = io.StringIO(), io.StringIO()
    argv = [*args, "--state-dir", str(tmp_path / "state")]
    if no_status:
        argv.append("--no-status")
    rc = ledgerwatch.run(argv, out=out, err=err)
    return rc, out.getvalue(), err.getvalue()


# --------------------------------------------------------------------------
# JSONL / tail strategy
# --------------------------------------------------------------------------


def test_jsonl_cold_start_emits_all(tmp_path):
    f = tmp_path / "bus.jsonl"
    f.write_text('{"seq":1}\n{"seq":2}\n')
    rc, out, _ = lw(tmp_path, str(f))
    assert rc == 0
    assert '{"seq":1}' in out and '{"seq":2}' in out


def test_jsonl_append_emits_only_new(tmp_path):
    f = tmp_path / "bus.jsonl"
    f.write_text('{"seq":1}\n')
    lw(tmp_path, str(f))  # prime cursor
    f.write_text('{"seq":1}\n{"seq":2}\n')
    _, out, _ = lw(tmp_path, str(f))
    assert out == '{"seq":2}\n'


def test_jsonl_noop_emits_nothing(tmp_path):
    f = tmp_path / "bus.jsonl"
    f.write_text('{"seq":1}\n')
    lw(tmp_path, str(f))
    rc, out, _ = lw(tmp_path, str(f))
    assert out == "" and rc == NO_CHANGE  # silence is pinned to the no-change code


def test_jsonl_rotation_truncate_reemits(tmp_path):
    # FAILURE MODE: file rotated/truncated to shorter, different content.
    # A naive byte offset would emit nothing (missed update) or slice mid-line.
    f = tmp_path / "bus.jsonl"
    f.write_text('{"seq":1}\n{"seq":2}\n{"seq":3}\n')
    lw(tmp_path, str(f))  # cursor now past 3 lines
    f.write_text('{"seq":99}\n')  # rotated: shorter + new first line
    _, out, _ = lw(tmp_path, str(f))
    assert '{"seq":99}' in out  # the update is NOT missed


def test_jsonl_inplace_rewrite_same_size_detected(tmp_path):
    # FAILURE MODE: in-place rewrite that keeps byte length identical.
    # Pure offset == size would report "no change" and miss it.
    f = tmp_path / "bus.jsonl"
    f.write_text("aaaa\nbbbb\n")
    lw(tmp_path, str(f))
    f.write_text("xxxx\nyyyy\n")  # same 10 bytes, different content
    _, out, _ = lw(tmp_path, str(f))
    assert "xxxx" in out and "yyyy" in out


def test_jsonl_partial_line_emitted_once_when_complete(tmp_path):
    # FAILURE MODE: writer caught mid-append. Must not emit a half record, and
    # must not lose it once the newline lands.
    f = tmp_path / "bus.jsonl"
    f.write_text('{"seq":1}\n')
    lw(tmp_path, str(f))
    with open(f, "a") as fh:
        fh.write('{"seq":2}')  # partial, no newline
    _, out, _ = lw(tmp_path, str(f))
    assert out == ""  # partial not emitted
    with open(f, "a") as fh:
        fh.write("\n")  # completes the record
    _, out, _ = lw(tmp_path, str(f))
    assert out == '{"seq":2}\n'  # emitted exactly once


def test_jsonl_select_filters_by_field(tmp_path):
    f = tmp_path / "bus.jsonl"
    f.write_text('{"component":"auth","m":1}\n{"component":"store","m":2}\n')
    _, out, _ = lw(tmp_path, str(f), "--select", "component=auth")
    assert '"component":"auth"' in out
    assert '"component":"store"' not in out


def test_jsonl_select_membership_multiple_values(tmp_path):
    f = tmp_path / "bus.jsonl"
    f.write_text('{"c":"a"}\n{"c":"b"}\n{"c":"z"}\n')
    _, out, _ = lw(tmp_path, str(f), "--select", "c=a,b")
    assert '"c":"a"' in out and '"c":"b"' in out and '"c":"z"' not in out


def test_jsonl_empty_file_no_output(tmp_path):
    f = tmp_path / "bus.jsonl"
    f.write_text("")
    rc, out, _ = lw(tmp_path, str(f))
    assert rc == NO_CHANGE and out == ""


# --------------------------------------------------------------------------
# Markdown / section strategy (status, roadmap, plan, design, ledger docs)
# --------------------------------------------------------------------------

THREE_SECTIONS = (
    "## Alpha\nalpha-body-one\n\n"
    "## Bravo\nbravo-body-one\n\n"
    "## Charlie\ncharlie-body-one\n"
)


def test_md_cold_start_emits_all_sections(tmp_path):
    f = tmp_path / "STATUS.md"
    f.write_text(THREE_SECTIONS)
    _, out, _ = lw(tmp_path, str(f))
    assert "Alpha" in out and "Bravo" in out and "Charlie" in out


def test_md_intra_document_edit_detected(tmp_path):
    # FAILURE MODE & the whole reason this strategy exists: an edit to the FIRST
    # section, with later sections unchanged, must be caught. A tail cursor on a
    # multi-section file would never see it.
    f = tmp_path / "STATUS.md"
    f.write_text(THREE_SECTIONS)
    lw(tmp_path, str(f))
    f.write_text(THREE_SECTIONS.replace("alpha-body-one", "alpha-body-TWO"))
    _, out, _ = lw(tmp_path, str(f))
    assert "alpha-body-TWO" in out
    assert "[changed]" in out
    assert "bravo-body-one" not in out  # untouched sections not re-emitted
    assert "charlie-body-one" not in out


def test_md_append_within_section_detected(tmp_path):
    f = tmp_path / "ledger.md"
    f.write_text(THREE_SECTIONS)
    lw(tmp_path, str(f))
    f.write_text(
        THREE_SECTIONS.replace("bravo-body-one\n", "bravo-body-one\nbravo-entry-2\n")
    )
    _, out, _ = lw(tmp_path, str(f))
    assert "bravo-entry-2" in out
    assert "Alpha" not in out and "Charlie" not in out


def test_md_noop_emits_nothing(tmp_path):
    f = tmp_path / "plan.md"
    f.write_text(THREE_SECTIONS)
    lw(tmp_path, str(f))
    rc, out, _ = lw(tmp_path, str(f))
    assert out == "" and rc == NO_CHANGE


def test_md_new_section_emitted_as_new(tmp_path):
    f = tmp_path / "roadmap.md"
    f.write_text(THREE_SECTIONS)
    lw(tmp_path, str(f))
    f.write_text(THREE_SECTIONS + "\n## Delta\ndelta-body\n")
    _, out, _ = lw(tmp_path, str(f))
    assert "Delta" in out and "[new]" in out
    assert "Alpha" not in out


def test_md_removed_section_reported(tmp_path):
    # FAILURE MODE: a section disappearing is an event the monitor must surface,
    # not silently swallow.
    f = tmp_path / "design.md"
    f.write_text(THREE_SECTIONS)
    lw(tmp_path, str(f))
    f.write_text(
        "## Alpha\nalpha-body-one\n\n## Bravo\nbravo-body-one\n"
    )  # Charlie gone
    _, out, _ = lw(tmp_path, str(f))
    assert "Charlie" in out and "[removed]" in out


def test_md_reorder_is_not_a_false_change(tmp_path):
    # FAILURE MODE (false positive): reordering identical sections must NOT be
    # reported as a change, or every reshuffle floods context.
    f = tmp_path / "STATUS.md"
    f.write_text(THREE_SECTIONS)
    lw(tmp_path, str(f))
    reordered = (
        "## Charlie\ncharlie-body-one\n\n"
        "## Alpha\nalpha-body-one\n\n"
        "## Bravo\nbravo-body-one\n"
    )
    f.write_text(reordered)
    _, out, _ = lw(tmp_path, str(f))
    assert out == ""


def test_md_duplicate_headings_disambiguated(tmp_path):
    f = tmp_path / "notes.md"
    f.write_text("## Notes\nfirst-note\n\n## Notes\nsecond-note\n")
    lw(tmp_path, str(f))
    # edit only the second "## Notes"
    f.write_text("## Notes\nfirst-note\n\n## Notes\nsecond-note-EDIT\n")
    _, out, _ = lw(tmp_path, str(f))
    assert "second-note-EDIT" in out
    assert "first-note\n" not in out


def test_md_preamble_tracked(tmp_path):
    f = tmp_path / "doc.md"
    f.write_text("preamble-line\n\n## Alpha\nalpha-body\n")
    lw(tmp_path, str(f))
    f.write_text("preamble-line-EDITED\n\n## Alpha\nalpha-body\n")
    _, out, _ = lw(tmp_path, str(f))
    assert "preamble-line-EDITED" in out
    assert "(preamble)" in out
    assert "Alpha" not in out


# --------------------------------------------------------------------------
# diff fallback strategy
# --------------------------------------------------------------------------


def test_txt_uses_diff_cold_start(tmp_path):
    f = tmp_path / "notes.txt"
    f.write_text("line-a\nline-b\n")
    _, out, _ = lw(tmp_path, str(f))
    assert "+line-a" in out and "+line-b" in out


def test_txt_edit_emits_hunk_only(tmp_path):
    f = tmp_path / "notes.txt"
    f.write_text("line-a\nline-b\nline-c\n")
    lw(tmp_path, str(f))
    f.write_text("line-a\nline-B-EDIT\nline-c\n")
    _, out, _ = lw(tmp_path, str(f))
    assert "-line-b" in out and "+line-B-EDIT" in out
    assert "line-a" not in out.replace(" line-a", "")  # context line only, no +/-


def test_txt_noop_emits_nothing(tmp_path):
    f = tmp_path / "notes.txt"
    f.write_text("x\n")
    lw(tmp_path, str(f))
    rc, out, _ = lw(tmp_path, str(f))
    assert out == "" and rc == NO_CHANGE


def test_unknown_extension_falls_back_to_diff(tmp_path):
    f = tmp_path / "thing.conf"
    f.write_text("k=v\n")
    _, out, _ = lw(tmp_path, str(f))
    assert "+k=v" in out  # unified-diff marker => diff strategy used


# --------------------------------------------------------------------------
# State handling & robustness
# --------------------------------------------------------------------------


def test_corrupt_state_is_cold_start(tmp_path):
    f = tmp_path / "bus.jsonl"
    f.write_text('{"seq":1}\n')
    state_dir = tmp_path / "state"
    state_dir.mkdir()
    key = ledgerwatch.sha1(str(f.resolve()).encode())
    (state_dir / f"{key}.json").write_text("{ not json")
    _, out, _ = lw(tmp_path, str(f))
    assert '{"seq":1}' in out  # garbage state -> re-emit, no crash


def test_reset_reemits_everything(tmp_path):
    f = tmp_path / "bus.jsonl"
    f.write_text('{"seq":1}\n{"seq":2}\n')
    lw(tmp_path, str(f))
    rc_noreset, out_noreset, _ = lw(tmp_path, str(f))
    assert out_noreset == "" and rc_noreset == NO_CHANGE
    _, out_reset, _ = lw(tmp_path, str(f), "--reset")
    assert '{"seq":1}' in out_reset and '{"seq":2}' in out_reset


def test_missing_file_exit_2(tmp_path):
    rc, out, err = lw(tmp_path, str(tmp_path / "nope.jsonl"))
    assert rc == ERROR and out == "" and "no such file" in err


def test_strategy_override_forces_tail_on_md(tmp_path):
    f = tmp_path / "STATUS.md"
    f.write_text("## Alpha\nbody\n")
    _, out, _ = lw(tmp_path, str(f), "--strategy", "tail")
    assert "## Alpha" in out and "=====" not in out  # raw lines, not section markers


def test_strategy_change_between_runs_cold_starts(tmp_path):
    f = tmp_path / "data.log"
    f.write_text("one\ntwo\n")
    lw(tmp_path, str(f))  # tail by extension
    _, out, _ = lw(tmp_path, str(f), "--strategy", "diff")  # strategy switched
    assert "+one" in out and "+two" in out  # full re-emit under new strategy


def test_default_exit_codes_are_grep_style(tmp_path):
    f = tmp_path / "bus.jsonl"
    f.write_text('{"seq":1}\n')
    rc_changed, _, _ = lw(tmp_path, str(f))  # default, no flag
    assert rc_changed == CHANGED  # 0
    rc_noop, _, _ = lw(tmp_path, str(f))
    assert rc_noop == NO_CHANGE  # 1


def test_no_status_collapses_change_codes_to_zero(tmp_path):
    f = tmp_path / "bus.jsonl"
    f.write_text('{"seq":1}\n')
    rc_changed, _, _ = lw(tmp_path, str(f), no_status=True)
    assert rc_changed == 0
    rc_noop, _, _ = lw(tmp_path, str(f), no_status=True)
    assert rc_noop == 0  # no-change is no longer a nonzero exit


def test_no_status_still_reports_errors(tmp_path):
    # Negative control: opting out of change-status must NOT mask real errors.
    rc, _, err = lw(tmp_path, str(tmp_path / "nope.jsonl"), no_status=True)
    assert rc == ERROR and "no such file" in err


def test_select_warns_on_non_tail_strategy(tmp_path):
    f = tmp_path / "STATUS.md"
    f.write_text("## Alpha\nbody\n")
    _, _, err = lw(tmp_path, str(f), "--select", "x=1")
    assert "ignored" in err


def state_files(tmp_path, suffix=".json"):
    sd = tmp_path / "state"
    if not sd.exists():
        return []
    return [n for n in os.listdir(sd) if n.endswith(suffix)]


# --------------------------------------------------------------------------
# Run-mode signal (cold / resync / incremental)
# --------------------------------------------------------------------------


def test_mode_cold_announced_on_first_run(tmp_path):
    f = tmp_path / "bus.jsonl"
    f.write_text('{"seq":1}\n')
    _, _, err = lw(tmp_path, str(f))
    assert "mode=cold" in err


def test_mode_incremental_is_quiet(tmp_path):
    f = tmp_path / "bus.jsonl"
    f.write_text('{"seq":1}\n')
    lw(tmp_path, str(f))
    f.write_text('{"seq":1}\n{"seq":2}\n')
    _, _, err = lw(tmp_path, str(f))
    assert "mode=" not in err  # a clean incremental delta says nothing


def test_mode_resync_announced_on_rotation(tmp_path):
    # The signal that matters: a rotated file re-emits a baseline, and the
    # consumer is told NOT to read it as N fresh events.
    f = tmp_path / "bus.jsonl"
    f.write_text('{"seq":1}\n{"seq":2}\n')
    lw(tmp_path, str(f))
    f.write_text('{"seq":99}\n')
    _, _, err = lw(tmp_path, str(f))
    assert "mode=resync" in err


def test_mode_section_never_resyncs(tmp_path):
    # Content-addressed strategy cannot lose its place; second run is always
    # incremental even after a sweeping edit.
    f = tmp_path / "STATUS.md"
    f.write_text(THREE_SECTIONS)
    lw(tmp_path, str(f))
    f.write_text(
        THREE_SECTIONS.replace("alpha-body-one", "X").replace("bravo-body-one", "Y")
    )
    _, _, err = lw(tmp_path, str(f))
    assert "mode=" not in err


def test_mode_section_cold_on_corrupt_state(tmp_path):
    f = tmp_path / "STATUS.md"
    f.write_text(THREE_SECTIONS)
    sd = tmp_path / "state"
    sd.mkdir()
    key = ledgerwatch.sha1(str(f.resolve()).encode())
    (sd / f"{key}.json").write_text("{ corrupt")
    _, out, err = lw(tmp_path, str(f))
    assert "Alpha" in out and "mode=cold" in err


# --------------------------------------------------------------------------
# --dry-run (peek without advancing the cursor)
# --------------------------------------------------------------------------


def test_dry_run_does_not_advance_tail(tmp_path):
    f = tmp_path / "bus.jsonl"
    f.write_text('{"seq":1}\n')
    _, dry, _ = lw(tmp_path, str(f), "--dry-run")
    assert dry == '{"seq":1}\n'
    _, real, _ = lw(tmp_path, str(f))  # still emitted: dry-run did not commit
    assert real == '{"seq":1}\n'


def test_dry_run_is_idempotent(tmp_path):
    f = tmp_path / "bus.jsonl"
    f.write_text('{"seq":1}\n{"seq":2}\n')
    _, a, _ = lw(tmp_path, str(f), "--dry-run")
    _, b, _ = lw(tmp_path, str(f), "--dry-run")
    assert a == b and '{"seq":2}' in a


def test_dry_run_diff_does_not_write_shadow(tmp_path):
    # FAILURE MODE: a peek must not silently consume the change by advancing the
    # shadow, or the next real run would miss it.
    f = tmp_path / "notes.txt"
    f.write_text("a\nb\n")
    _, dry, _ = lw(tmp_path, str(f), "--dry-run")
    assert "+a" in dry
    assert not state_files(tmp_path, ".shadow")  # no shadow written
    _, real, _ = lw(tmp_path, str(f))
    assert "+a" in real and "+b" in real  # full change still seen


def test_dry_run_section_does_not_persist(tmp_path):
    f = tmp_path / "STATUS.md"
    f.write_text(THREE_SECTIONS)
    lw(tmp_path, str(f))  # commit baseline
    f.write_text(THREE_SECTIONS.replace("alpha-body-one", "alpha-X"))
    _, dry, _ = lw(tmp_path, str(f), "--dry-run")
    assert "alpha-X" in dry
    _, real, _ = lw(tmp_path, str(f))
    assert "alpha-X" in real  # dry-run did not record the new hash


def test_dry_run_exit_code_still_reflects_change(tmp_path):
    f = tmp_path / "bus.jsonl"
    f.write_text('{"seq":1}\n')
    rc, _, _ = lw(tmp_path, str(f), "--dry-run")
    assert rc == CHANGED  # change detected even though not committed


# --------------------------------------------------------------------------
# --json (structured envelope)
# --------------------------------------------------------------------------


def test_json_tail_envelope(tmp_path):
    f = tmp_path / "bus.jsonl"
    f.write_text('{"seq":1}\n')
    _, out, _ = lw(tmp_path, str(f), "--json")
    env = json.loads(out)
    assert env["strategy"] == "tail"
    assert env["mode"] == "cold"
    assert env["changed"] is True
    assert env["lines"] == ['{"seq":1}']


def test_json_section_envelope(tmp_path):
    f = tmp_path / "STATUS.md"
    f.write_text("## Alpha\nbody-a\n")
    _, out, _ = lw(tmp_path, str(f), "--json")
    env = json.loads(out)
    assert env["strategy"] == "section"
    sec = env["sections"][0]
    assert sec["key"] == "## Alpha" and sec["kind"] == "new" and "body-a" in sec["body"]


def test_json_diff_envelope(tmp_path):
    f = tmp_path / "notes.txt"
    f.write_text("hello\n")
    _, out, _ = lw(tmp_path, str(f), "--json")
    env = json.loads(out)
    assert env["strategy"] == "diff" and "hello" in env["diff"]


def test_json_noop_is_silent_with_no_change_code(tmp_path):
    # --json stays silent on an idle tick too (cheap); the exit code, not an
    # envelope, carries the no-change status.
    f = tmp_path / "bus.jsonl"
    f.write_text('{"seq":1}\n')
    lw(tmp_path, str(f), "--json")
    rc, out, _ = lw(tmp_path, str(f), "--json")
    assert out == "" and rc == NO_CHANGE


def test_json_escapes_quotes_newlines_unicode(tmp_path):
    # FAILURE MODE: structured output must survive bodies containing characters
    # that would otherwise break a naive concatenation.
    f = tmp_path / "STATUS.md"
    f.write_text('## A\nquote " backslash \\ unicode ✓ done\n')
    _, out, _ = lw(tmp_path, str(f), "--json")
    env = json.loads(out)  # must not raise
    body = env["sections"][0]["body"]
    assert '"' in body and "✓" in body and "\\" in body


def test_json_mode_reports_resync(tmp_path):
    f = tmp_path / "bus.jsonl"
    f.write_text('{"seq":1}\n{"seq":2}\n')
    lw(tmp_path, str(f), "--json")
    f.write_text('{"seq":9}\n')
    _, out, _ = lw(tmp_path, str(f), "--json")
    assert json.loads(out)["mode"] == "resync"


def test_json_dry_run_combines(tmp_path):
    f = tmp_path / "bus.jsonl"
    f.write_text('{"seq":1}\n')
    _, out1, _ = lw(tmp_path, str(f), "--json", "--dry-run")
    _, out2, _ = lw(tmp_path, str(f), "--json")  # real run still sees the line
    assert json.loads(out1)["lines"] == ['{"seq":1}']
    assert json.loads(out2)["lines"] == ['{"seq":1}']


# --------------------------------------------------------------------------
# --prune (drop cursors for vanished files) and the no-file-argument paths
# --------------------------------------------------------------------------


def test_prune_removes_stale_cursor(tmp_path):
    f = tmp_path / "gone.jsonl"
    f.write_text("x\n")
    lw(tmp_path, str(f))
    assert state_files(tmp_path)  # cursor exists
    f.unlink()
    rc, _, err = lw(tmp_path, "--prune")
    assert rc == 0 and not state_files(tmp_path) and "pruned" in err


def test_prune_keeps_live_cursor(tmp_path):
    f = tmp_path / "live.jsonl"
    f.write_text("x\n")
    lw(tmp_path, str(f))
    lw(tmp_path, "--prune")
    assert state_files(tmp_path)  # still present


def test_prune_removes_orphan_shadow(tmp_path):
    f = tmp_path / "doc.conf"  # diff strategy -> writes a shadow
    f.write_text("k=v\n")
    lw(tmp_path, str(f))
    assert state_files(tmp_path, ".shadow")
    f.unlink()
    lw(tmp_path, "--prune")
    assert not state_files(tmp_path, ".shadow")


def test_prune_ignores_unparseable_state(tmp_path):
    # FAILURE MODE: prune must not crash on, or wrongly delete, a state file it
    # cannot read (it can't prove what such a file tracks).
    sd = tmp_path / "state"
    sd.mkdir()
    (sd / "deadbeef.json").write_text("{ not json")
    rc, _, _ = lw(tmp_path, "--prune")
    assert rc == 0 and (sd / "deadbeef.json").exists()


def test_prune_on_missing_state_dir_is_noop(tmp_path):
    rc, _, err = lw(tmp_path, "--prune")  # state dir never created
    assert rc == 0 and "pruned 0" in err


def test_missing_file_argument_without_prune_errors(tmp_path):
    rc, out, err = lw(tmp_path)  # no file, no --prune
    assert rc == 2 and out == "" and "required" in err


# --------------------------------------------------------------------------
# delta-validate (default, tail) + --validate (opt-in full-file scan)
# --------------------------------------------------------------------------


def test_delta_validate_warns_on_invalid_line_in_delta(tmp_path):
    # FAILURE MODE: a torn/corrupt line in the emitted delta must be surfaced on
    # stderr (with its line number), not silently passed through.
    f = tmp_path / "bus.jsonl"
    f.write_text('{"seq":1}\n')
    lw(tmp_path, str(f))  # prime cursor
    with open(f, "a") as fh:
        fh.write("{not json}\n")
    rc, out, err = lw(tmp_path, str(f))
    assert rc == 0  # the line is still emitted (delta status unchanged)
    assert "{not json}" in out
    assert "bus.jsonl:2 invalid JSON in delta" in err  # absolute line number


def test_delta_validate_warns_even_when_select_drops_the_bad_line(tmp_path):
    # The silent-miss this guards: --select does json.loads and drops non-JSON,
    # so without the warning a corrupt line would vanish with no trace.
    f = tmp_path / "bus.jsonl"
    f.write_text('{"c":"a"}\n')
    lw(tmp_path, str(f), "--select", "c=a")  # prime
    with open(f, "a") as fh:
        fh.write("torn{oops\n")
    rc, out, err = lw(tmp_path, str(f), "--select", "c=a")
    assert rc == NO_CHANGE and out == ""  # select dropped it → nothing emitted
    assert "invalid JSON in delta" in err  # ...but it was NOT silent


def test_delta_validate_clean_delta_is_silent(tmp_path):
    f = tmp_path / "bus.jsonl"
    f.write_text('{"seq":1}\n')
    lw(tmp_path, str(f))
    with open(f, "a") as fh:
        fh.write('{"seq":2}\n')
    _, _, err = lw(tmp_path, str(f))
    assert "invalid" not in err  # no false positive on valid JSON


def test_delta_validate_only_applies_to_tail(tmp_path):
    # section/diff strategies have no JSON-per-line semantics → no validation.
    f = tmp_path / "notes.md"
    f.write_text("# H\nnot json at all\n")
    _, _, err = lw(tmp_path, str(f))
    assert "invalid JSON" not in err


def test_validate_clean_file_exits_0(tmp_path):
    f = tmp_path / "l.jsonl"
    f.write_text('{"a":1}\n{"b":2}\n')
    rc, out, err = lw(tmp_path, str(f), "--validate")
    assert rc == 0 and out == "" and "valid" in err


def test_validate_reports_interior_corruption_exit_3(tmp_path):
    # The case ledgerwrite's trailing-only heal CANNOT catch: a bad line in the
    # middle of the file (e.g. from a botched sed -i).
    f = tmp_path / "l.jsonl"
    f.write_text('{"a":1}\nCORRUPTED MIDDLE\n{"c":3}\n')
    rc, _, err = lw(tmp_path, str(f), "--validate")
    assert rc == 3
    assert "l.jsonl:2 invalid JSON" in err


def test_validate_flags_unterminated_final_line_exit_3(tmp_path):
    f = tmp_path / "l.jsonl"
    with open(f, "w") as fh:
        fh.write('{"a":1}\n{"b":2')  # no trailing newline
    rc, _, err = lw(tmp_path, str(f), "--validate")
    assert rc == 3
    assert "unterminated final line" in err


def test_validate_ignores_blank_lines(tmp_path):
    f = tmp_path / "l.jsonl"
    f.write_text('{"a":1}\n\n   \n{"b":2}\n')  # blank + whitespace-only lines
    rc, _, _ = lw(tmp_path, str(f), "--validate")
    assert rc == 0  # blanks are not corruption


def test_validate_bounds_output(tmp_path):
    f = tmp_path / "l.jsonl"
    f.write_text("".join("nope\n" for _ in range(50)))  # 50 invalid lines
    rc, _, err = lw(tmp_path, str(f), "--validate")
    assert rc == 3
    assert "more invalid line(s)" in err  # capped + summarized, not 50 lines
    assert err.count("invalid JSON") <= 21  # cap (20) + the summary line


def test_validate_json_envelope(tmp_path):
    f = tmp_path / "l.jsonl"
    f.write_text('{"a":1}\nbad\n')
    rc, out, _ = lw(tmp_path, str(f), "--validate", "--json")
    assert rc == 3
    env = json.loads(out)
    assert env["mode"] == "validate" and env["valid"] is False
    assert env["invalid_count"] == 1 and env["invalid_lines"][0]["line"] == 2


def test_validate_missing_file_exits_2(tmp_path):
    rc, _, err = lw(tmp_path, str(tmp_path / "nope.jsonl"), "--validate")
    assert rc == 2 and "no such file" in err


# --------------------------------------------------------------------------
# --project (fold-to-latest-per-id)
#
# RED-first: the recipe this mode replaces (dev/todos-and-considerations-ledger-
# readme.md lines 222-237, deleted in the same commit) did
# `latest[r["id"]] = r` and died with `KeyError: 'id'` on the first id-less
# entry — reproduced against the real ledger before this mode existed. Every
# arm below is therefore built around id-less entries being ROUTINE, not fatal.
# --------------------------------------------------------------------------

PROJECT_OK = 0


def test_project_folds_to_latest_per_id(tmp_path):
    f = tmp_path / "l.jsonl"
    f.write_text(
        '{"seq":1,"id":"TC-1","status":"open"}\n'
        '{"seq":2,"id":"TC-2","status":"open"}\n'
        '{"seq":3,"id":"TC-1","status":"resolved"}\n'
    )
    rc, out, _ = lw(tmp_path, str(f), "--project")
    rows = [json.loads(line) for line in out.splitlines()]
    assert rc == PROJECT_OK
    assert [r["id"] for r in rows] == ["TC-1", "TC-2"]  # sorted by id
    # TC-1's LATEST entry wins; its earlier "open" state is history.
    assert rows[0]["status"] == "resolved" and rows[0]["seq"] == 3


def test_project_entry_without_id_is_bucketed_not_fatal(tmp_path):
    """The exact input that killed the old recipe with KeyError: 'id'."""
    f = tmp_path / "l.jsonl"
    f.write_text(
        '{"seq":1,"id":"TC-1","status":"open"}\n'
        '{"seq":2,"note":"a decision-trail entry with no id"}\n'
        '{"seq":3,"id":"TC-1","status":"resolved"}\n'
    )
    rc, out, err = lw(tmp_path, str(f), "--project")
    assert rc == PROJECT_OK  # not an error, and not a traceback
    assert "unfoldable (no id): 1" in err
    assert len(out.splitlines()) == 1


def test_project_all_entries_id_less_is_not_an_error(tmp_path):
    """dev/steward/steward-ledger.jsonl's real shape: no entry has an `id`."""
    f = tmp_path / "steward.jsonl"
    f.write_text('{"seq":1,"decision":"a"}\n{"seq":2,"decision":"b"}\n')
    rc, out, err = lw(tmp_path, str(f), "--project")
    assert rc == PROJECT_OK
    assert out == ""  # nothing foldable
    assert "0 id(s) folded" in err and "unfoldable (no id): 2" in err


def test_project_bucket_is_printed_even_when_zero(tmp_path):
    """Always printed, so 'nothing to fold' can never look like 'all folded'."""
    f = tmp_path / "l.jsonl"
    f.write_text('{"seq":1,"id":"TC-1"}\n')
    _, _, err = lw(tmp_path, str(f), "--project")
    assert "unfoldable (no id): 0" in err


def test_project_does_not_advance_or_create_the_cursor(tmp_path):
    f = tmp_path / "l.jsonl"
    f.write_text('{"seq":1,"id":"TC-1"}\n')
    lw(tmp_path, str(f), "--project")
    state_dir = tmp_path / "state"
    assert not state_dir.exists()  # no cursor created at all
    # A subsequent watch run therefore still sees the whole file as new.
    rc, out, _ = lw(tmp_path, str(f))
    assert rc == CHANGED and '"id":"TC-1"' in out.replace(" ", "")


def test_project_does_not_mutate_the_ledger(tmp_path):
    f = tmp_path / "l.jsonl"
    original = '{"seq":1,"id":"TC-1"}\n{"seq":2}\n'
    f.write_text(original)
    lw(tmp_path, str(f), "--project")
    assert f.read_text() == original


def test_project_malformed_line_is_bucketed_separately(tmp_path):
    f = tmp_path / "l.jsonl"
    f.write_text('{"seq":1,"id":"TC-1"}\nnot json at all\n[1,2,3]\n')
    rc, out, err = lw(tmp_path, str(f), "--project")
    assert rc == PROJECT_OK
    assert "malformed: 2" in err  # torn line + a non-object JSON value
    assert len(out.splitlines()) == 1


def test_project_blank_lines_are_not_entries(tmp_path):
    f = tmp_path / "l.jsonl"
    f.write_text('{"seq":1,"id":"TC-1"}\n\n   \n{"seq":2,"id":"TC-2"}\n')
    _, out, err = lw(tmp_path, str(f), "--project")
    assert len(out.splitlines()) == 2
    assert "from 2 entries" in err and "malformed: 0" in err


def test_project_empty_file_is_an_empty_projection(tmp_path):
    f = tmp_path / "l.jsonl"
    f.write_text("")
    rc, out, err = lw(tmp_path, str(f), "--project")
    assert rc == PROJECT_OK and out == ""
    assert "0 id(s) folded" in err and "unfoldable (no id): 0" in err


def test_project_json_envelope(tmp_path):
    f = tmp_path / "l.jsonl"
    f.write_text(
        '{"seq":1,"id":"TC-1","status":"open"}\n'
        '{"seq":2,"note":"no id"}\n'
        '{"seq":3,"id":"TC-1","status":"resolved"}\n'
    )
    rc, out, err = lw(tmp_path, str(f), "--project", "--json")
    env = json.loads(out)
    assert rc == PROJECT_OK
    assert env["mode"] == "project"
    assert env["entries"] == 3 and env["folded_ids"] == 1
    assert env["unfoldable_no_id"] == 1 and env["malformed"] == 0
    assert env["latest"][0]["status"] == "resolved"
    assert "unfoldable (no id): 1" in err  # bucket printed in --json mode too


def test_project_missing_file_exits_2(tmp_path):
    rc, _, err = lw(tmp_path, str(tmp_path / "nope.jsonl"), "--project")
    assert rc == ERROR and "no such file" in err


def test_project_non_string_and_empty_ids(tmp_path):
    f = tmp_path / "l.jsonl"
    f.write_text('{"seq":1,"id":7}\n{"seq":2,"id":""}\n{"seq":3,"id":null}\n')
    _, out, err = lw(tmp_path, str(f), "--project")
    assert len(out.splitlines()) == 1  # id 7 folds under the key "7"
    assert "unfoldable (no id): 2" in err  # "" and null are not usable ids


# --------------------------------------------------------------------------
# --project rulings (the ruling registry)
#
# RED-first. The projection key here CANNOT be `id`: dev/steward/steward-
# ledger.jsonl is the ledger that holds most of this repo's rulings and 107/107
# of its entries have NO `id` — and adding one was explicitly forbidden (T1b).
# So the address is `id` when present, else `seq-N`, and the RULING SIGNAL is
# `kind == "decision"`, the only classifier present on 193/193 entries across
# all three ledgers. `status` is read for exactly ONE literal value, `open`,
# which the todos-ledger README declares itself; every other status is REPORTED
# VERBATIM and never interpreted, because inventing a status vocabulary is a
# refused move in this effort (T1b) and the README's own terminal vocabulary
# (`done`/`wont-do`/`superseded`) matches ZERO real entries.
# --------------------------------------------------------------------------


def test_rulings_a_decision_entry_is_ruled(tmp_path):
    f = tmp_path / "l.jsonl"
    f.write_text('{"seq":1,"kind":"decision","summary":"ruled it"}\n')
    rc, out, err = lw(tmp_path, str(f), "--project", "rulings")
    rows = [json.loads(line) for line in out.splitlines()]
    assert rc == PROJECT_OK
    assert rows[0]["verdict"] == "ruled" and rows[0]["key"] == "seq-1"
    assert "ruled: 1" in err


def test_rulings_an_open_entry_is_unruled(tmp_path):
    f = tmp_path / "l.jsonl"
    f.write_text('{"seq":1,"id":"TC-3","kind":"consideration","status":"open"}\n')
    rc, out, err = lw(tmp_path, str(f), "--project", "rulings")
    rows = [json.loads(line) for line in out.splitlines()]
    assert rc == PROJECT_OK
    assert rows[0]["verdict"] == "unruled" and rows[0]["key"] == "TC-3"
    assert "unruled: 1" in err


def test_rulings_unclassifiable_entry_is_bucketed_not_dropped(tmp_path):
    """`resolved` is NOT read as a ruling: it is not in any declared vocabulary.

    Real instance: TC-7 walked open -> in_progress -> converged-pending-hitl ->
    resolved without a single `kind: decision` entry. The registry must SAY it
    cannot classify that, not guess.
    """
    f = tmp_path / "l.jsonl"
    f.write_text('{"seq":1,"id":"TC-7","kind":"reconcile","status":"resolved"}\n')
    rc, out, err = lw(tmp_path, str(f), "--project", "rulings")
    rows = [json.loads(line) for line in out.splitlines()]
    assert rc == PROJECT_OK
    assert rows[0]["verdict"] == "unclassified"  # emitted, never dropped
    assert rows[0]["status"] == "resolved"       # echoed verbatim, uninterpreted
    assert "unclassified: 1" in err


def test_rulings_every_bucket_is_printed_even_when_zero(tmp_path):
    """An empty ledger must still name all four buckets, or "nothing to
    classify" reads as "everything classified" (the T1b bucket lesson)."""
    f = tmp_path / "l.jsonl"
    f.write_text("")
    rc, out, err = lw(tmp_path, str(f), "--project", "rulings")
    assert rc == PROJECT_OK and out == ""
    for bucket in ("ruled: 0", "unruled: 0", "unclassified: 0", "malformed: 0"):
        assert bucket in err


def test_rulings_malformed_line_is_bucketed_separately(tmp_path):
    f = tmp_path / "l.jsonl"
    f.write_text('{"seq":1,"kind":"decision"}\nnot json at all\n[1,2,3]\n')
    rc, out, err = lw(tmp_path, str(f), "--project", "rulings")
    assert rc == PROJECT_OK
    assert "malformed: 2" in err  # torn line + a non-object JSON value
    assert len(out.splitlines()) == 1


def test_rulings_id_less_entries_are_keyed_by_seq(tmp_path):
    """The steward ledger's real shape: no entry has an `id`, and adding one is
    banned. Every entry must still be addressable — by `seq-N`."""
    f = tmp_path / "steward.jsonl"
    f.write_text(
        '{"seq":1,"kind":"decision","summary":"a"}\n'
        '{"seq":2,"kind":"decision","summary":"b"}\n'
    )
    rc, out, err = lw(tmp_path, str(f), "--project", "rulings")
    rows = [json.loads(line) for line in out.splitlines()]
    assert rc == PROJECT_OK
    assert [r["key"] for r in rows] == ["seq-1", "seq-2"]
    assert all(r["verdict"] == "ruled" for r in rows)
    assert "keyed by seq (no id): 2" in err


def test_rulings_folds_to_latest_per_id(tmp_path):
    """Reuses T1b's fold: the item's current state is its LAST entry."""
    f = tmp_path / "l.jsonl"
    f.write_text(
        '{"seq":1,"id":"TC-1","kind":"consideration","status":"open"}\n'
        '{"seq":2,"id":"TC-1","kind":"decision","status":"closed"}\n'
    )
    _, out, err = lw(tmp_path, str(f), "--project", "rulings")
    rows = [json.loads(line) for line in out.splitlines()]
    assert len(rows) == 1 and rows[0]["seq"] == 2
    assert rows[0]["verdict"] == "ruled" and rows[0]["status"] == "closed"
    assert "1 item(s) from 2 entries" in err


def test_rulings_a_ruling_is_not_undone_by_a_later_observation(tmp_path):
    """A ruling, once recorded, stays recorded — this is the whole point. The
    fold decides the item's CURRENT fields; it does not un-rule the item."""
    f = tmp_path / "l.jsonl"
    f.write_text(
        '{"seq":1,"id":"TC-1","kind":"decision","summary":"ruled"}\n'
        '{"seq":2,"id":"TC-1","kind":"observation","status":"open"}\n'
    )
    _, out, _ = lw(tmp_path, str(f), "--project", "rulings")
    rows = [json.loads(line) for line in out.splitlines()]
    assert rows[0]["verdict"] == "ruled"      # NOT flipped back to unruled
    assert rows[0]["ruling_seqs"] == [1]      # and it says exactly where


def test_rulings_status_case_variants_both_read_as_open(tmp_path):
    """The real todos ledger carries BOTH `open` (46) and `OPEN` (9)."""
    f = tmp_path / "l.jsonl"
    f.write_text(
        '{"seq":1,"id":"TC-1","kind":"question","status":"open"}\n'
        '{"seq":2,"id":"TC-2","kind":"question","status":"OPEN"}\n'
    )
    _, _, err = lw(tmp_path, str(f), "--project", "rulings")
    assert "unruled: 2" in err


def test_rulings_ordering_is_deterministic(tmp_path):
    """Sorted by (ruled, unruled, unclassified) then key — same bytes every run."""
    f = tmp_path / "l.jsonl"
    f.write_text(
        '{"seq":1,"id":"TC-9","kind":"observation","status":"watching"}\n'
        '{"seq":2,"id":"TC-2","kind":"question","status":"open"}\n'
        '{"seq":3,"id":"TC-1","kind":"decision"}\n'
        '{"seq":4,"id":"TC-0","kind":"question","status":"open"}\n'
    )
    _, out_a, _ = lw(tmp_path, str(f), "--project", "rulings")
    _, out_b, _ = lw(tmp_path, str(f), "--project", "rulings")
    keys = [json.loads(line)["key"] for line in out_a.splitlines()]
    assert out_a == out_b
    assert keys == ["TC-1", "TC-0", "TC-2", "TC-9"]


def test_rulings_entry_with_neither_id_nor_seq_still_gets_a_key(tmp_path):
    f = tmp_path / "l.jsonl"
    f.write_text('{"kind":"decision","summary":"no id, no seq"}\n')
    _, out, err = lw(tmp_path, str(f), "--project", "rulings")
    rows = [json.loads(line) for line in out.splitlines()]
    assert rows[0]["key"] == "line-1"  # addressable by file position, not dropped
    assert "keyed by seq (no id): 1" in err


def test_rulings_does_not_mutate_the_ledger_or_the_cursor(tmp_path):
    f = tmp_path / "l.jsonl"
    original = '{"seq":1,"kind":"decision"}\n{"seq":2,"kind":"observation"}\n'
    f.write_text(original)
    lw(tmp_path, str(f), "--project", "rulings")
    assert f.read_text() == original
    assert not (tmp_path / "state").exists()  # read/report only


def test_rulings_json_envelope(tmp_path):
    f = tmp_path / "l.jsonl"
    f.write_text(
        '{"seq":1,"id":"TC-1","kind":"decision","status":"closed"}\n'
        '{"seq":2,"kind":"observation"}\n'
        '{"seq":3,"id":"TC-2","kind":"question","status":"open"}\n'
        "junk\n"
    )
    rc, out, err = lw(tmp_path, str(f), "--project", "rulings", "--json")
    env = json.loads(out)
    assert rc == PROJECT_OK
    assert env["mode"] == "project" and env["projection"] == "rulings"
    assert env["entries"] == 4 and env["items"] == 3
    assert env["ruled"] == 1 and env["unruled"] == 1 and env["unclassified"] == 1
    assert env["malformed"] == 1 and env["keyed_by_seq"] == 1
    assert [r["key"] for r in env["rows"]] == ["TC-1", "TC-2", "seq-2"]
    assert "unclassified: 1" in err  # buckets printed in --json mode too


def test_rulings_missing_file_exits_2(tmp_path):
    rc, _, err = lw(tmp_path, str(tmp_path / "nope.jsonl"), "--project", "rulings")
    assert rc == ERROR and "no such file" in err


def test_rulings_summary_is_not_capped(tmp_path):
    """T1b refused a --summary length cap; a registry row must not silently
    truncate the one line a reader uses to recognise the ruling."""
    long = "x" * 400
    f = tmp_path / "l.jsonl"
    f.write_text(json.dumps({"seq": 1, "kind": "decision", "summary": long}) + "\n")
    _, out, _ = lw(tmp_path, str(f), "--project", "rulings")
    assert json.loads(out.splitlines()[0])["summary"] == long


# --- --project argument back-compatibility (T1b's bare form must survive) ---


def test_project_bare_flag_still_means_fold_to_latest(tmp_path):
    f = tmp_path / "l.jsonl"
    f.write_text('{"seq":1,"id":"TC-1","status":"open"}\n')
    rc, out, err = lw(tmp_path, str(f), "--project")
    assert rc == PROJECT_OK and "unfoldable (no id): 0" in err
    assert json.loads(out.splitlines()[0])["id"] == "TC-1"


def test_project_named_latest_is_the_same_as_the_bare_flag(tmp_path):
    f = tmp_path / "l.jsonl"
    f.write_text('{"seq":1,"id":"TC-1","status":"open"}\n')
    _, bare, _ = lw(tmp_path, str(f), "--project")
    _, named, _ = lw(tmp_path, str(f), "--project", "latest")
    assert bare == named


def test_project_file_first_argument_form_still_works(tmp_path):
    """`ledgerwatch --project FILE` — the form documented in the ledger README.
    argparse would otherwise eat the path as the projection NAME."""
    f = tmp_path / "l.jsonl"
    f.write_text('{"seq":1,"id":"TC-1","status":"open"}\n')
    rc, out, err = lw(tmp_path, "--project", str(f))
    assert rc == PROJECT_OK and "unfoldable (no id): 0" in err
    assert json.loads(out.splitlines()[0])["id"] == "TC-1"


def test_project_unknown_projection_name_exits_2(tmp_path):
    f = tmp_path / "l.jsonl"
    f.write_text('{"seq":1,"id":"TC-1"}\n')
    rc, _, err = lw(tmp_path, str(f), "--project", "nonsense")
    assert rc == ERROR and "unknown projection" in err

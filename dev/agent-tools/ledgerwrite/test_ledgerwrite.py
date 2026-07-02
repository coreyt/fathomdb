"""Tests for ledgerwrite. Run: python3 -m pytest -q in this directory.

Covers the properties that matter: every write is one line of valid JSON that
round-trips; ts/seq are stamped and monotonic; arbitrary fields and refs land;
reserved keys can't be clobbered; --dry-run touches nothing; the ledger body is
never read; and the output is consumable by ledgerwatch's --select.
"""

import importlib.util
import json
import os
import subprocess
import sys
from datetime import datetime

import pytest

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location(
    "ledgerwrite", os.path.join(HERE, "ledgerwrite.py")
)
lw = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lw)


class Cap:
    """Minimal stdout/stderr capture with a .getvalue()."""

    def __init__(self):
        self.buf = []

    def write(self, s):
        self.buf.append(s)

    def getvalue(self):
        return "".join(self.buf)


def call(argv):
    out, err = Cap(), Cap()
    rc = lw.run(argv, out=out, err=err)
    return rc, out.getvalue(), err.getvalue()


def read_lines(path):
    with open(path, encoding="utf-8") as fh:
        return [ln for ln in fh.read().splitlines() if ln]


# --- happy path -------------------------------------------------------------


def test_cold_create_then_append(tmp_path):
    ledger = str(tmp_path / "l.jsonl")
    rc, out, _ = call([ledger, "--kind", "decision", "--summary", "first"])
    assert rc == 0
    lines = read_lines(ledger)
    assert len(lines) == 1
    rec = json.loads(lines[0])
    assert rec["kind"] == "decision"
    assert rec["summary"] == "first"
    # echoed record on stdout matches what landed
    assert json.loads(out) == rec

    rc, _, _ = call([ledger, "--kind", "drift", "--summary", "second"])
    assert rc == 0
    assert len(read_lines(ledger)) == 2


def test_every_line_is_valid_single_line_json(tmp_path):
    ledger = str(tmp_path / "l.jsonl")
    # newlines / quotes / unicode in content must not break the JSONL stream
    call([ledger, "--kind", "note", "--summary", 'a "quoted"\nmulti-line ☃'])
    lines = read_lines(ledger)
    assert len(lines) == 1  # embedded newline did not create a second line
    rec = json.loads(lines[0])
    assert rec["summary"] == 'a "quoted"\nmulti-line ☃'


def test_ts_is_iso_utc_with_z(tmp_path):
    ledger = str(tmp_path / "l.jsonl")
    call([ledger, "--kind", "note", "--summary", "x"])
    rec = json.loads(read_lines(ledger)[0])
    assert rec["ts"].endswith("Z")
    assert "T" in rec["ts"]


# --- seq --------------------------------------------------------------------


def test_seq_is_monotonic(tmp_path):
    ledger = str(tmp_path / "l.jsonl")
    for _ in range(3):
        call([ledger, "--kind", "note", "--summary", "x"])
    seqs = [json.loads(ln)["seq"] for ln in read_lines(ledger)]
    assert seqs == [1, 2, 3]


def test_no_seq_omits_field(tmp_path):
    ledger = str(tmp_path / "l.jsonl")
    call([ledger, "--kind", "note", "--summary", "x", "--no-seq"])
    rec = json.loads(read_lines(ledger)[0])
    assert "seq" not in rec


def test_corrupt_seq_counter_recovers(tmp_path):
    ledger = str(tmp_path / "l.jsonl")
    seq_file = str(tmp_path / "l.jsonl.seq")
    with open(seq_file, "w", encoding="utf-8") as fh:
        fh.write("not-a-number")
    call([ledger, "--kind", "note", "--summary", "x"])
    rec = json.loads(read_lines(ledger)[0])
    assert rec["seq"] == 1


# --- fields / refs / body ---------------------------------------------------


def test_arbitrary_fields_land_and_are_selectable(tmp_path):
    ledger = str(tmp_path / "l.jsonl")
    call(
        [
            ledger,
            "--kind",
            "drift",
            "--summary",
            "roadmap stale",
            "--field",
            "surface=roadmap",
            "--field",
            "decider=hitl",
        ]
    )
    rec = json.loads(read_lines(ledger)[0])
    assert rec["surface"] == "roadmap"
    assert rec["decider"] == "hitl"


def test_field_value_may_contain_equals(tmp_path):
    ledger = str(tmp_path / "l.jsonl")
    call([ledger, "--kind", "note", "--summary", "x", "--field", "expr=a=b=c"])
    rec = json.loads(read_lines(ledger)[0])
    assert rec["expr"] == "a=b=c"


def test_refs_become_a_list(tmp_path):
    ledger = str(tmp_path / "l.jsonl")
    call(
        [
            ledger,
            "--kind",
            "reconcile",
            "--summary",
            "x",
            "--ref",
            "git:abc123",
            "--ref",
            "seq:7",
        ]
    )
    rec = json.loads(read_lines(ledger)[0])
    assert rec["refs"] == ["git:abc123", "seq:7"]


def test_body_included(tmp_path):
    ledger = str(tmp_path / "l.jsonl")
    call([ledger, "--kind", "note", "--summary", "x", "--body", "the long story"])
    rec = json.loads(read_lines(ledger)[0])
    assert rec["body"] == "the long story"


def test_reserved_key_cannot_be_clobbered_by_field(tmp_path):
    ledger = str(tmp_path / "l.jsonl")
    rc, _, err = call(
        [ledger, "--kind", "note", "--summary", "x", "--field", "kind=evil"]
    )
    assert rc == 0
    rec = json.loads(read_lines(ledger)[0])
    assert rec["kind"] == "note"  # the flag wins, not the --field
    assert "ignored (reserved key" in err


# --- dry-run ----------------------------------------------------------------


def test_dry_run_writes_nothing(tmp_path):
    ledger = str(tmp_path / "l.jsonl")
    rc, out, _ = call([ledger, "--kind", "note", "--summary", "x", "--dry-run"])
    assert rc == 0
    assert not os.path.exists(ledger)  # nothing written
    assert json.loads(out)["kind"] == "note"  # but the record was echoed
    assert not os.path.exists(str(tmp_path / "l.jsonl.seq"))  # counter untouched


def test_quiet_suppresses_echo(tmp_path):
    ledger = str(tmp_path / "l.jsonl")
    rc, out, _ = call([ledger, "--kind", "note", "--summary", "x", "--quiet"])
    assert rc == 0
    assert out == ""
    assert len(read_lines(ledger)) == 1


# --- error handling ---------------------------------------------------------


def test_missing_file_arg_exits_2(tmp_path):
    rc, _, err = call(["--kind", "note", "--summary", "x"])
    assert rc == 2
    assert "ledger file argument required" in err


def test_missing_kind_exits_2(tmp_path):
    ledger = str(tmp_path / "l.jsonl")
    rc, _, err = call([ledger, "--summary", "x"])
    assert rc == 2
    assert "--kind" in err


def test_empty_summary_exits_2(tmp_path):
    ledger = str(tmp_path / "l.jsonl")
    rc, _, err = call([ledger, "--kind", "note", "--summary", "   "])
    assert rc == 2
    assert "--summary" in err


def test_bad_field_exits_2(tmp_path):
    ledger = str(tmp_path / "l.jsonl")
    rc, _, err = call([ledger, "--kind", "note", "--summary", "x", "--field", "nope"])
    assert rc == 2
    assert "key=value" in err
    assert not os.path.exists(ledger)  # rejected before any write


def test_missing_parent_dir_exits_2(tmp_path):
    ledger = str(tmp_path / "nope" / "l.jsonl")
    rc, _, err = call([ledger, "--kind", "note", "--summary", "x"])
    assert rc == 2
    assert "no such directory" in err


# --- seq counter placement --------------------------------------------------


def test_seq_counter_lives_beside_the_ledger(tmp_path):
    ledger = str(tmp_path / "l.jsonl")
    call([ledger, "--kind", "note", "--summary", "x"])
    assert os.path.exists(ledger + ".seq")  # the counter is intrinsic to the ledger


def test_no_state_dir_option(tmp_path):
    """--state-dir was removed: it let one ledger keep two independent counters
    (different dirs) and thus reuse a seq. The counter is now always beside the
    ledger, so the option no longer exists."""
    ledger = str(tmp_path / "l.jsonl")
    with pytest.raises(SystemExit):  # argparse rejects the unknown option
        lw.run([ledger, "--kind", "note", "--summary", "x", "--state-dir", "x"])


def test_negative_seq_counter_restarts_at_1(tmp_path):
    ledger = str(tmp_path / "l.jsonl")
    with open(ledger + ".seq", "w", encoding="utf-8") as fh:
        fh.write("-5")  # nonsensical (e.g. hand-tampered) counter
    call([ledger, "--kind", "note", "--summary", "x"])
    assert json.loads(read_lines(ledger)[0])["seq"] == 1


# --- torn-line heal (pread last byte) ---------------------------------------


def test_heals_torn_trailing_line_from_a_foreign_writer(tmp_path):
    """A prior writer left an unterminated line (crash / bad shell append). Our
    record must NOT merge onto it: the fragment stays its own line and our
    record lands clean, valid, and independently selectable — with a warning."""
    ledger = str(tmp_path / "l.jsonl")
    with open(ledger, "w", encoding="utf-8") as fh:
        fh.write('{"seq":1,"kind":"good","summary":"complete"}\n')
        fh.write('{"seq":2,"kind":"torn","summ')  # torn: no trailing newline
    rc, _, err = call([ledger, "--kind", "decision", "--summary", "mine"])
    assert rc == 0
    assert "healed a torn trailing line" in err
    lines = read_lines(ledger)
    assert len(lines) == 3  # fragment isolated onto its own line, ours separate
    assert lines[1] == '{"seq":2,"kind":"torn","summ'  # fragment left intact
    assert json.loads(lines[2])["summary"] == "mine"  # our record is valid JSON


def test_no_heal_when_last_line_is_properly_terminated(tmp_path):
    """A normal newline-terminated ledger must NOT gain a spurious blank line."""
    ledger = str(tmp_path / "l.jsonl")
    call([ledger, "--kind", "note", "--summary", "one"])
    rc, _, err = call([ledger, "--kind", "note", "--summary", "two"])
    assert rc == 0
    assert "healed" not in err
    with open(ledger, encoding="utf-8") as fh:
        assert "\n\n" not in fh.read()  # no empty line inserted


def test_no_heal_on_empty_or_new_file(tmp_path):
    ledger = str(tmp_path / "l.jsonl")
    _, _, err = call([ledger, "--kind", "note", "--summary", "x"])
    assert "healed" not in err  # size 0 → nothing to heal


# --- failed write leaves the ledger untouched -------------------------------


def test_failed_write_removes_the_empty_file_it_created(tmp_path, monkeypatch):
    """If the append fails after open() created the file (e.g. flock errors on a
    filesystem that raises), the empty file we created is removed so a rejected
    call leaves the ledger untouched."""
    if lw.fcntl is None:
        pytest.skip("no fcntl to fault-inject")
    ledger = str(tmp_path / "l.jsonl")

    def boom(*_a, **_k):
        raise OSError("simulated lock failure")

    monkeypatch.setattr(lw.fcntl, "flock", boom)
    rc, _, err = call([ledger, "--kind", "note", "--summary", "x"])
    assert rc == 2
    assert "write failed" in err
    assert not os.path.exists(ledger)  # no 0-byte file left behind


def test_failed_write_preserves_a_preexisting_ledger(tmp_path, monkeypatch):
    """A failure must never delete a ledger that already had content."""
    if lw.fcntl is None:
        pytest.skip("no fcntl to fault-inject")
    ledger = str(tmp_path / "l.jsonl")
    call([ledger, "--kind", "note", "--summary", "first"])  # real prior content

    def boom(*_a, **_k):
        raise OSError("simulated lock failure")

    monkeypatch.setattr(lw.fcntl, "flock", boom)
    rc, _, _ = call([ledger, "--kind", "note", "--summary", "second"])
    assert rc == 2
    assert os.path.exists(ledger)  # not deleted
    assert len(read_lines(ledger)) == 1  # prior content intact


# --- failure modes: the characteristic silent-corruption risks --------------
# (ledgerwrite's analog of ledgerwatch's "never miss an update" suite: never
#  reuse a seq, never interleave a line, never corrupt or drop a record.)


def test_concurrent_writers_no_seq_reuse_or_interleave(tmp_path):
    """The headline robustness claim: N processes appending at once must
    serialize under the flock — no lost line, no interleaved bytes, no two
    records sharing a seq. This is why ledgerwrite takes a lock at all."""
    if lw.fcntl is None:  # pragma: no cover - the lock is the guarantee
        pytest.skip("no fcntl on this platform; flock guarantee not available")
    ledger = str(tmp_path / "l.jsonl")
    script = os.path.join(HERE, "ledgerwrite.py")
    n = 24
    procs = [
        subprocess.Popen(
            [
                sys.executable,
                script,
                ledger,
                "--kind",
                "note",
                "--summary",
                f"writer-{i}",
                "--quiet",
            ]
        )
        for i in range(n)
    ]
    assert all(p.wait() == 0 for p in procs)
    lines = read_lines(ledger)
    assert len(lines) == n  # no write was lost or merged into another
    recs = [json.loads(ln) for ln in lines]  # every line is intact valid JSON
    seqs = sorted(r["seq"] for r in recs)
    assert seqs == list(range(1, n + 1))  # no duplicate, no gap
    summaries = {r["summary"] for r in recs}
    assert summaries == {f"writer-{i}" for i in range(n)}  # every writer landed
    # ts is stamped inside the lock, so it is non-decreasing in seq order.
    by_seq = [r["ts"] for r in sorted(recs, key=lambda r: r["seq"])]
    assert by_seq == sorted(by_seq)


def test_append_preserves_foreign_prior_content(tmp_path):
    """Appending must never touch or corrupt a line ledgerwrite did not write."""
    ledger = str(tmp_path / "l.jsonl")
    foreign = '{"ts":"2020-01-01T00:00:00.000Z","seq":99,"kind":"seed","summary":"hand-written"}'
    with open(ledger, "w", encoding="utf-8") as fh:
        fh.write(foreign + "\n")
    call([ledger, "--kind", "note", "--summary", "appended", "--no-seq"])
    lines = read_lines(ledger)
    assert lines[0] == foreign  # byte-for-byte untouched
    assert json.loads(lines[1])["summary"] == "appended"


def test_written_line_is_newline_terminated(tmp_path):
    """A record must never leave a torn trailing line for a reader to recover."""
    ledger = str(tmp_path / "l.jsonl")
    call([ledger, "--kind", "note", "--summary", "x"])
    with open(ledger, encoding="utf-8") as fh:
        assert fh.read().endswith("\n")


def test_ts_parses_as_iso8601(tmp_path):
    ledger = str(tmp_path / "l.jsonl")
    call([ledger, "--kind", "note", "--summary", "x"])
    ts = json.loads(read_lines(ledger)[0])["ts"]
    parsed = datetime.fromisoformat(ts.replace("Z", "+00:00"))
    assert parsed.tzinfo is not None  # timezone-aware (UTC)


def test_write_target_is_a_directory_exits_2(tmp_path):
    """An I/O failure at write time is a clean exit 2, not a crash."""
    ledger = str(tmp_path / "l.jsonl")
    os.mkdir(ledger)  # the ledger path is a directory → open('a') fails
    rc, _, err = call([ledger, "--kind", "note", "--summary", "x"])
    assert rc == 2
    assert "write failed" in err


@pytest.mark.parametrize("reserved", ["ts", "seq", "kind", "summary", "refs", "body"])
def test_no_reserved_key_can_be_clobbered_by_field(tmp_path, reserved):
    """Every reserved key is flag-owned; a --field of the same name is dropped."""
    ledger = str(tmp_path / "l.jsonl")
    rc, _, err = call(
        [ledger, "--kind", "note", "--summary", "x", "--field", f"{reserved}=evil"]
    )
    assert rc == 0
    rec = json.loads(read_lines(ledger)[0])
    assert rec.get(reserved) != "evil"
    assert f"--field {reserved}" in err


def test_real_ref_and_body_win_over_colliding_field(tmp_path):
    """For refs/body the real flag value must WIN over a clobbering --field —
    not merely be absent (the parametrized test above only proved absence)."""
    ledger = str(tmp_path / "l.jsonl")
    rc, _, _ = call(
        [
            ledger,
            "--kind",
            "note",
            "--summary",
            "x",
            "--ref",
            "git:real",
            "--field",
            "refs=evil",
            "--body",
            "real body",
            "--field",
            "body=evil body",
        ]
    )
    assert rc == 0
    rec = json.loads(read_lines(ledger)[0])
    assert rec["refs"] == ["git:real"]
    assert rec["body"] == "real body"


def test_dry_run_then_real_write_starts_at_seq_1(tmp_path):
    """A dry-run must not consume a seq the next real write needs."""
    ledger = str(tmp_path / "l.jsonl")
    call([ledger, "--kind", "note", "--summary", "peek", "--dry-run"])
    call([ledger, "--kind", "note", "--summary", "real"])
    assert json.loads(read_lines(ledger)[0])["seq"] == 1


def test_empty_field_value_is_allowed(tmp_path):
    ledger = str(tmp_path / "l.jsonl")
    rc, _, _ = call([ledger, "--kind", "note", "--summary", "x", "--field", "tag="])
    assert rc == 0
    assert json.loads(read_lines(ledger)[0])["tag"] == ""


def test_field_with_empty_key_exits_2(tmp_path):
    ledger = str(tmp_path / "l.jsonl")
    rc, _, err = call([ledger, "--kind", "note", "--summary", "x", "--field", "=v"])
    assert rc == 2
    assert "non-empty" in err
    assert not os.path.exists(ledger)


def test_body_and_fields_escape_special_chars(tmp_path):
    ledger = str(tmp_path / "l.jsonl")
    call(
        [
            ledger,
            "--kind",
            "note",
            "--summary",
            "x",
            "--body",
            'line1\nline2 "q" ☃',
            "--field",
            "path=/a\\b",
        ]
    )
    lines = read_lines(ledger)
    assert len(lines) == 1  # embedded newline in body did not split the record
    rec = json.loads(lines[0])
    assert rec["body"] == 'line1\nline2 "q" ☃'
    assert rec["path"] == "/a\\b"

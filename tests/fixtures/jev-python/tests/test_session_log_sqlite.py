"""The session-log schema is real SQL and the fixture loads (REQ-SAG-0146, D6).

Python's stdlib ``sqlite3`` executes ``schema.sql`` and loads the ``fields{}``
fixture the way the documented loader rule says: ``INSERT OR IGNORE`` into
``sessions``, ``question_sets`` and ``payloads``, plain ``INSERT`` into
``answers``.
"""

import json
import sqlite3
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[4]
SCHEMA = ROOT / "crates/sc-decision-schema/src/session_log/schema.sql"
FIXTURE = ROOT / "tests/fixtures/decision-schema/session-log/fields.jsonl"


def fixture_rows():
    return [json.loads(line) for line in FIXTURE.read_text().splitlines()]


def load(connection, fields):
    connection.execute(
        "INSERT OR IGNORE INTO sessions (session_id, session_created_at) VALUES (?, ?)",
        (fields["session_id"], fields["session_created_at"]),
    )
    connection.execute(
        "INSERT OR IGNORE INTO question_sets (questions_sha256, question_ids, first_seen_at)"
        " VALUES (?, ?, ?)",
        (fields["questions_sha256"], json.dumps(fields["question_ids"]), fields["ts"]),
    )
    connection.execute(
        "INSERT OR IGNORE INTO payloads (payload_sha256, questions_sha256, first_seen_at)"
        " VALUES (?, ?, ?)",
        (fields["payload_sha256"], fields["questions_sha256"], fields["ts"]),
    )
    answers_json = fields["answers_json"]
    connection.execute(
        "INSERT INTO answers (answer_id, schema_version, session_id, questions_sha256,"
        " payload_sha256, decision_schema, repo, project, request_sha256, ts, operation, backend,"
        " model_id, send_outcome, error_code, answers_json)"
        " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        (
            fields["answer_id"],
            fields["schema_version"],
            fields["session_id"],
            fields["questions_sha256"],
            fields["payload_sha256"],
            fields["decision_schema"],
            fields["repo"],
            fields["project"],
            fields["request_sha256"],
            fields["ts"],
            fields["operation"],
            fields["backend"],
            fields["model_id"],
            fields["send_outcome"],
            fields["error_code"],
            None if answers_json is None else json.dumps(answers_json),
        ),
    )


@pytest.fixture
def connection():
    db = sqlite3.connect(":memory:")
    db.execute("PRAGMA foreign_keys = ON")
    db.executescript(SCHEMA.read_text())
    yield db
    db.close()


def test_fixture_loads_into_sqlite(connection):
    rows = fixture_rows()
    assert len(rows) == 7
    for fields in rows:
        load(connection, fields)

    count = lambda sql: connection.execute(sql).fetchone()[0]
    assert count("SELECT COUNT(*) FROM answers") == 7
    assert count("SELECT COUNT(*) FROM sessions") == 4
    assert count("SELECT COUNT(*) FROM question_sets") == 2
    assert count("SELECT COUNT(*) FROM payloads") == 3

    # The two rows of one session keep their own project.
    projects = connection.execute(
        "SELECT project FROM answers WHERE session_id = ? ORDER BY ts",
        (rows[0]["session_id"],),
    ).fetchall()
    assert projects == [("billing",), ("docs-site",)]

    # A failed send before any body was built has a model and no request digest.
    model, request = connection.execute(
        "SELECT model_id, request_sha256 FROM answers WHERE error_code = 'KEY_MISSING'"
    ).fetchone()
    assert model == "gpt-6-luna" and request is None

    # A row with no project keeps NULL.
    assert count("SELECT COUNT(*) FROM answers WHERE project IS NULL") == 1


def test_check_constraints_reject_inconsistent_rows(connection):
    ok = fixture_rows()[0]
    load(connection, ok)
    for change in (
        {"send_outcome": "failed", "answer_id": "01JC4Z8M3K5N7P9Q1R2S3T4V70"},
        {"error_code": "NETWORK_ERROR", "answer_id": "01JC4Z8M3K5N7P9Q1R2S3T4V71"},
        {"answers_json": None, "answer_id": "01JC4Z8M3K5N7P9Q1R2S3T4V72"},
        {"operation": "think", "answer_id": "01JC4Z8M3K5N7P9Q1R2S3T4V73"},
        {"send_outcome": "maybe", "answer_id": "01JC4Z8M3K5N7P9Q1R2S3T4V74"},
    ):
        with pytest.raises(sqlite3.IntegrityError):
            load(connection, {**ok, **change})


def test_same_questions_against_different_state(connection):
    """`questions_sha256` groups sends of one question set across states.

    This is the owner's query: for one question set, list each state (each
    ``payload_sha256``) with the answers it got, per model.
    """
    for fields in fixture_rows():
        load(connection, fields)
    questions = fixture_rows()[0]["questions_sha256"]
    rows = connection.execute(
        "SELECT payload_sha256, model_id, send_outcome FROM answers"
        " WHERE questions_sha256 = ? ORDER BY ts",
        (questions,),
    ).fetchall()
    payloads = {payload for payload, _, _ in rows}
    assert len(rows) == 4 and len(payloads) == 2, rows

    # The same question set seen through its payloads (one row per state).
    states = connection.execute(
        "SELECT p.payload_sha256, COUNT(a.answer_id) FROM payloads p"
        " JOIN answers a ON a.payload_sha256 = p.payload_sha256"
        " WHERE p.questions_sha256 = ? GROUP BY p.payload_sha256"
        " ORDER BY p.payload_sha256",
        (questions,),
    ).fetchall()
    assert sorted(count for _, count in states) == [1, 3]

    # A send can only name a payload under its own question set.
    row = {**fixture_rows()[0], "answer_id": "01JC4Z8M3K5N7P9Q1R2S3T4V7Z"}
    row["questions_sha256"] = fixture_rows()[2]["questions_sha256"]
    with pytest.raises(sqlite3.IntegrityError):
        connection.execute("PRAGMA foreign_keys = ON")
        connection.execute(
            "INSERT INTO answers (answer_id, schema_version, session_id, questions_sha256,"
            " payload_sha256, decision_schema, repo, project, request_sha256, ts, operation,"
            " backend, model_id, send_outcome, error_code, answers_json)"
            " VALUES (?, 2, ?, ?, ?, 'canonical.v1', '/r', NULL, NULL, 't', 'judge', 'jev',"
            " 'm', 'failed', 'NETWORK_ERROR', NULL)",
            (row["answer_id"], row["session_id"], row["questions_sha256"], row["payload_sha256"]),
        )

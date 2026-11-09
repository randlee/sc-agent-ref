#!/bin/bash
cd "/Volumes/Extreme Pro/github/sc-agent-worktrees/sprint/r-7-decision-schema" || exit 1
echo "HEAD $(git rev-parse --short HEAD)"
run(){ echo "### $*"; "$@" 2>&1 | grep -E "test result:|passed|failed|error|^sc-|^├|^└|Finished|PASS|FAIL|ok$" | tail -6; echo "rc=${PIPESTATUS[0]}"; }
t(){ run cargo test "$@"; }
run cargo tree -p sc-decision-schema -e normal --depth 1
run bash .github/scripts/cargo-tree-gates.sh
run cargo build -p sc-decision-schema --lib
t -p sc-decision-schema --test judge
t -p sc-decision-schema --test ids
t -p sc-decision-schema --features test-util --test mock
t -p sc-decision-schema --test version -- test_known_versions_accepted_unknown_rejected --exact
t -p sc-decision-schema --test version -- test_schema_version_not_in_canonical_bytes --exact
t -p workspace-tests --test jev_cross_impl
t -p sc-jev-check --test cross_impl_branches
t -p sc-decision-schema --test llm -- test_llm_output_is_decision_answers --exact
t -p sc-jev-backends --test llm
t -p sc-decision-schema --test variants -- test_canonical_variant_is_identity --exact
t -p sc-decision-schema --test variants -- test_every_model_family_maps_to_a_known_schema --exact
t -p sc-decision-schema --test session_log -- test_session_log_row_matches_ddl --exact
t -p sc-decision-schema --test session_log -- test_fields_roundtrip --exact
t -p sc-decision-schema --test session_log -- test_unknown_or_missing_key_rejected --exact
run uv run --directory tests/fixtures/jev-python pytest -q tests/test_session_log_sqlite.py::test_fixture_loads_into_sqlite
t -p sc-agent-types --test ids -- test_project_label_limits --exact
t -p sc-agent-types --test models -- test_known_models_roundtrip --exact
t -p sc-agent-types --test models -- test_unknown_model_string_parses_as_other --exact
t -p sc-jev-backends --test llm -- test_unknown_model_override_is_sent_verbatim --exact
t -p sc-agent-types --test models -- test_latest_alias_lookup --exact
t -p sc-agent-types --test models -- test_alias_override_wins --exact
t -p sc-agent-types --test ids -- test_backend_catalog_roundtrip --exact
t -p sc-jev-check --features test-util --test models -- test_check_toml_models_override_reaches_backend --exact
t -p sc-jev-check --features test-util --test models -- test_unknown_model_alias_is_config_invalid --exact
t -p sc-decision-schema --features test-util --test mock -- test_model_id_default_from_catalog --exact
t -p sc-jev --test catalog -- test_jev_model_id_is_configured_model --exact
t -p sc-jev-backends --test llm -- test_llm_model_id_is_configured_model --exact
t -p sc-agent-types --test trace -- test_send_trace_first_write_wins --exact
run cargo hack check -p sc-agent-types --feature-powerset --no-dev-deps
run cargo hack check -p sc-decision-schema --feature-powerset --no-dev-deps
t -p sc-agent-types --test features
echo "### grep Proposed, r-7"; grep -c "Proposed, r-7" docs/architecture.md
for f in sc-jev-check sc-ai sc-jev sc-jev-backends; do t -p $f --features test-util; done
run just lint
run just test
for p in sc-agent-types sc-decision-schema sc-agent-secrets sc-agent-http sc-jev sc-jev-backends sc-jev-check sc-jev-host; do run cargo build -p "$p" --lib; done
run bash .github/scripts/check-exact-pins.sh
run bash scripts/sc-publish-dry-run.sh
echo DONE

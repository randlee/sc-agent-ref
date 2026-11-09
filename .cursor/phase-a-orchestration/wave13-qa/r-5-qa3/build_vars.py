import json, datetime, sys
D = "/Volumes/Extreme Pro/github/sc-agent/.cursor/phase-a-orchestration/wave13-qa/r-5-qa3"
url = sys.argv[1] if len(sys.argv) > 1 else "pending"

manifest = [
  {"agent":"rust-qa-agent","task_id":"a0cd4dda4056f7543","spawn_actor":"parent-orchestrator","fenced_json_received":True,"verdict":"PASS","findings":{"blocking":0,"important":0,"minor":0}},
  {"agent":"arch-qa","task_id":"acd66eeefacb1ad2c","spawn_actor":"parent-orchestrator","fenced_json_received":True,"verdict":"FAIL","findings":{"blocking":3,"important":1,"minor":0}},
  {"agent":"req-qa","task_id":"a169297eaf240bff8","spawn_actor":"parent-orchestrator","fenced_json_received":True,"verdict":"FAIL","findings":{"blocking":2,"important":0,"minor":1}},
  {"agent":"flaky-test-qa","task_id":"af6999ee037040c3f","spawn_actor":"parent-orchestrator","fenced_json_received":True,"verdict":"PASS","findings":{"blocking":0,"important":0,"minor":0}},
]
evidence = {"qa_pass":"qa-3","commit":"6972530","pr_comment_url":url,
            "coordinator_task_id":"cursor-quality-mgr","triage":{"ttl_paths":[]}}

blocking_md = "\n".join([
 "- **REQ-QA-001** (Blocking, req-qa): acceptance commands not executed in the req-qa session. Parent log `acceptance-6972530.log` shows every named command rc=0, but that is parent evidence and does not replace req-qa fenced JSON. Remedy: shell-capable req-qa re-run at 6972530 that records the `1 passed` lines.",
 "- **REQ-QA-002** (Blocking, req-qa): the 1.94.1 type annotation was not located in the req-qa session. Handoff notes place it at `crates/sc-decision-schema/src/jev/v1.rs:193` (`Pairs::<K, V>`). Remedy: req-qa confirms that line and the 1.94.1 compile result.",
 "- **ARCH-010** (Blocking, arch-qa): `docs/plans/phase-a-refine/sprint-r-5-jev-wire-envelope.md` adrs list (lines 42-56) omits ADR-SAG-0029. Remedy: add `ADR-SAG-0029` with its (c) row to the list.",
 "- **ARCH-011** (Blocking, arch-qa): `CheckResponse::into_answers` in `crates/sc-decision-schema/src/jev/v1.rs:760` maps response to DecisionAnswers, which ADR-SAG-0012 r-5 (`docs/architecture.md:1035-1039`) keeps in sc-jev. Remedy: move the mapping to `crates/sc-jev`, or land an owner-approved Active ADR-SAG-0012 r-5 amendment first. Not fixable inside review.",
 "- **ARCH-012** (Blocking, arch-qa): plan code sample for `into_answers` (sprint doc line 296) contradicts ADR-SAG-0012 r-5. Remedy: update the sample and deliverable text after the ARCH-011 decision.",
])
detailed_md = blocking_md + "\n" + "\n".join([
 "- **ARCH-013** (Important, arch-qa): `crates/sc-decision-schema/src/payload.rs:245` (`Question::from_json`) and `:520` (`PayloadBundle::decode`) still use `encode_failed` on parse failures. Remedy: route both through `decode_failed`, update the `Question::from_json` doc block (`payload.rs:229-231`). Keep `encode_failed` on encode-only sites.",
 "- **REQ-QA-003** (Minor, req-qa): `test_malformed_body_hint_is_fix_the_body` is not in the sprint acceptance table (sprint doc lines 300-311). Remedy: add `cargo test -p sc-decision-schema --test jev -- test_malformed_body_hint_is_fix_the_body --exact` to the NFR-SAG-0002 or boundary:jev row.",
 "- TODO scan (`.claude/skills/todo-triage/scripts/find_todos.py`, plus an independent grep of `*.rs`): 0 hits.",
])

vars_ = {
 "generated_at": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
 "qa_pass": "qa-3",
 "sprint_id": "r-5",
 "task_id": "r-5-qa3",
 "branch": "sprint/r-5-jev-wire-envelope",
 "commit": "6972530",
 "pr_number": "34",
 "verdict": "FAIL",
 "deliverables_complete": "3",
 "deliverables_total": "5",
 "deliverables_percent": "60",
 "merge_readiness": "not-ready",
 "merge_reason": "Finding gate FAIL: 5 Blocking, 1 Important, 1 Minor open. req-qa reports 60% deliverable completion. Spawn gate passed and CI is green, but the merge gate needs all reviewer findings closed.",
 "findings_blocking": "5",
 "findings_important": "1",
 "findings_minor": "1",
 "blocking_ids_json": json.dumps(["REQ-QA-001","REQ-QA-002","ARCH-010","ARCH-011","ARCH-012"]),
 "blocking_findings_md": blocking_md,
 "detailed_findings_md": detailed_md,
 "next_action": "triage_and_fix",
 "action_owner": "parent-orchestrator (cursor-orchestration)",
 "reviewer_spawn_gate": "pass",
 "reviewer_manifest_json": json.dumps(manifest),
 "evidence_chain_json": json.dumps(evidence),
 "missing_reviewers_json": "[]",
 "unparsed_reviewers_json": "[]",
}
with open(f"{D}/findings-report-vars.json","w") as f:
    json.dump(vars_, f, indent=1)
print("ok", vars_["generated_at"])

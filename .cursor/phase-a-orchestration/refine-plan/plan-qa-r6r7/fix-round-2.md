# PR 29 plan QA-2 fix round (qa-plan-r67-2, 5836f1b): 0B / 8I / 10M

Enforcer report: https://github.com/randlee/sc-agent/pull/29#issuecomment-6714019730
Triage records, committed on integrate/phase-a at a6ec032: .triage/phase-a-refine/findings/{RBP-F001..F014,RSH-001..004}.ttl. All of them promote to plan/phase-a-refine-r6.
Reviewer JSON: rust-best-practices-qa2.json and rust-service-hardening-qa2.json in this directory.
Owner rule: add no gates, receipts or docs-consistency checks. Of the remedies a reviewer offers, prefer the simpler one.

## Parent dispositions (apply exactly)
- **RBP-F001 ACCEPT.** The engine creates the SendTrace per send and passes it to the backend's judge call, so it is never carried on the cloneable JudgeRequest. Record why OnceLock is used in a rustdoc line.
- **RBP-F002 ACCEPT.** Use one validating constructor (or try_from) in sc-decision-schema, which both crates call. Add a QuestionKind {Choice, Score, Noul} with skip_serializing_if = is_choice, so the bytes stay identical.
- **RBP-F003 and RSH-003 ACCEPT.** OrderedMap rejects duplicate keys. Enforce criteria counts in the validating constructor. JevState::Object is OrderedMap<String, Value>. Test keys are unsorted. Drop the "verbatim" claim for nested Values.
- **RBP-F004 ACCEPT.** Add a millisecond timestamp newtype and an AnswerId newtype.
- **RBP-F005 ACCEPT the simpler option.** Drop the DecisionSchema trait until a second variant exists, and keep Canonical as plain functions. State that dispatch is static and keyed by SchemaFamily. This supersedes RBP-F011 (sealing) and RBP-F012 (fake-fallible input()): close both as no longer applicable. Update the ADR-SAG-0030 "Model-family variants" bullet.
- **RBP-F006, RBP-F014 and RSH-004 ACCEPT together.** HttpRequest carries a text body (String), with_sent_body takes impl Into<String>, and there is no Option or Result arm.
  - Sha256Digest::of documents its LF normalization. Rename it to of_text.
  - query_sha256 identifies LF-normalized text, and the claim "digest equals SHA-256 of the bytes sent" applies to request bodies only.
- **RBP-F007 PARTIAL.** The owner specified string alias lookup ("luna"/"haiku"), so keep ModelAliases::latest(&str) and with_override(&str). Remove ModelId::fallback_for and the unwrap_or_else default: the JudgeBackend::model_id default is used only by catalog backends, whose alias is always in the builtin table, and non-catalog backends must implement model_id. Keep released as is.
- **RBP-F008 ACCEPT.** NoulProbabilities { yes } uses deny_unknown_fields, and the response model is a ModelId. Skip the Probability newtype (minimal).
- **RBP-F009 ACCEPT.** ProjectLabel gets Display, FromStr, AsRef<str>, Hash and serde, matching ModelId.
- **RBP-F010 ACCEPT.** Each new error path carries a fix hint, and the CONFIG_INVALID remap preserves it.
- **RBP-F011 and RBP-F012 CLOSED, superseded by RBP-F005.**
- **RBP-F013 ACCEPT.** When log_context is unset, its default uses discover_main_repo_root(repo_root).
- **RSH-001 ACCEPT.** into_answers keeps two existing checks: the response model equals the request model, and the choice/score probability keys equal the criteria keys. Add the noul key-set and range cases to test_response_rules_rejected.
- **RSH-002 ACCEPT.** sc-jev compares the to_body() length to the descriptor's request_limit_bytes before sending, with InputTooLarge as today.

## Report
Push to plan/phase-a-refine-r6. Return fenced JSON: status, pushed SHA, and a per-id table (id → section changed, or closed-superseded).

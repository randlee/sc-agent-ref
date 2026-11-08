# sc-agent-types

Shared error, id, catalog, and `sc/subjects/1` types for the sc-agent workspace.

Every other sc-agent crate depends on these types: `JevError` and `ErrorCode`, the static backend catalog, the backend and model catalogs, `SendTrace`, digests, and the `sc/subjects/1` schema. The judge contract and canonical payload live in `sc-decision-schema`. Default features: `secret`, `obs`; optional: `git`.

Part of the [sc-agent](https://github.com/randlee/sc-agent) workspace. See the
repository `docs/` for architecture and requirements. Licensed under MIT.

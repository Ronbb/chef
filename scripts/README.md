# Shared author tooling

`check-curriculum.mjs <chef-binary> <release.json> <course-root> [...]` checks an immutable release against exact lesson revisions from independent course repositories. It recursively discovers author source files, rejects divergent copies of the same revision, materializes a temporary source set, and delegates structural/placement/grading validation to the real Chef CLI. Temporary files are removed on success and failure. No database, provider call, media import or publication occurs.

The source resolver regression runs with `node --test scripts/check-curriculum.test.mjs`. Successful offline checks do not establish media registration, real audio decoding or production publication readiness; those remain server-side gates.

Backup/restore, authenticated encryption, health inspection, Qwen legacy French pilot tools and offline alignment now belong here. `pnpm test:ops` runs synthetic regressions; `BRIOCHE_BACKUP_DOCKER_TEST=1` enables an isolated real Docker restore exercise and never targets production. Existing serialized names and explicit target options remain compatible during product isolation work.

Invoke from the intended product workspace. Alignment private output/model paths resolve against that current workspace; `CHEF_WORKSPACE_ROOT` explicitly overrides it for callers elsewhere. Model identity and pinned runtime JSON resolve beside the shared source. Do not copy private caches or API keys into Chef. French samples and voice defaults are compatibility fixtures; these scripts do not claim Cantonese capability.

Product scripts may be tiny compatibility entries using `product-cli.mjs`; import remains side-effect free and CLI calls forward to the fixed shared source. No automatic backup scheduling, off-disk retention, paid synthesis or publication is introduced by extraction.

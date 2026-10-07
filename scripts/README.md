# Shared author tooling

`check-curriculum.mjs <chef-binary> <release.json> <course-root> [...]` checks an immutable release against exact lesson revisions from independent course repositories. It recursively discovers author source files, rejects divergent copies of the same revision, materializes a temporary source set, and delegates structural/placement/grading validation to the real Chef CLI. Temporary files are removed on success and failure. No database, provider call, media import or publication occurs.

The source resolver regression runs with `node --test scripts/check-curriculum.test.mjs`. Successful offline checks do not establish media registration, real audio decoding or production publication readiness; those remain server-side gates.

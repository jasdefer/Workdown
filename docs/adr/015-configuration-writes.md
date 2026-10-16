# ADR-015: Configuration writes re-serialize the file; content is the source of truth, not formatting

**Status:** Accepted
**Date:** 2026-10-16

## Context

The web app writes two configuration files: `views.yaml` through
`crate::operations::view_write` and `schema.yaml` through
`crate::operations::schema_write`. Both files are hand-edited YAML that
may carry comments, blank lines, compact `[a, b]` lists and quoting
choices. A writer has three options: splice the changed entry into the
original text by byte span (keeps everything else byte-for-byte, needs
a span-aware parser), use a comment-preserving YAML library (none in
Rust is mature enough to trust with the file the whole tool depends
on), or parse the file into a generic tree, change one entry and write
the tree back (loses comments and formatting, needs nothing new). The
choice was argued in `workdown-items/schema-editor-web-design.md`
(decisions 3 and 14) and is recorded here because it binds every
configuration writer, present and future.

## Decision

**A write re-serializes the whole file from a parsed form and writes
it back.** `views.yaml` is written from the typed model; `schema.yaml`
from a generic `serde_yaml` tree with the one entry the write is about
changed, because the typed schema has no writer and would have to turn
compiled expressions back into text. Either way content survives
exactly and formatting does not: comments are dropped, compact lists
come back one entry per line, quoting is normalized. The file is
the source of truth for *what it says*, and the tool treats how it is
laid out as its own to decide. Explanations belong in `description`
fields, which are content and therefore survive; the shipped default
`schema.yaml` carries none of the tutorial comments it once had, and
the schema guide holds them instead.

Two consequences follow and are also decided:

- **The editor is comment-blind.** It neither writes comments nor
  warns that a save will drop them. A per-save warning would be noise
  for the common case and could not say which comments go, since the
  tree never saw them.
- **Last write wins, per entry.** The server reads the file fresh on
  every write and replaces one entry, so two writers on one machine
  overwrite each other only on that entry. There is no content hash,
  no `409` on a changed file and no merge: same machine, same user,
  editing the file and the web app at once is not a case worth a
  mechanism.

What is *not* decided here: whether a write is refused or warns. That
is ADR-001's save-with-warning rule, applied by both writers the same
way: a candidate that would not load is refused and nothing is written;
one that loads but fails a cross-file check is written and its findings
reported.

## Consequences

- Any configuration writer added later — `resources.yaml`,
  `config.yaml` — follows the same shape and inherits the same loss of
  formatting, and needs no new parser.
- A project that wants comments in a configuration file keeps them by
  not saving that file from the web app. This repository's own
  `schema.yaml` is commented and will be flattened by its first browser
  save; that is accepted.
- Should a comment-preserving writer ever become worth having, it
  replaces the tree write behind the same operations without touching
  the refuse-or-warn contract or the endpoints.

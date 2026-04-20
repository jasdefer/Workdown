---
id: foundation
type: milestone
status: in_progress
title: Foundation
parent: phase-04-visualization
---

Prereq work for everything else in phase 04. Establishes the workspace layout and the `views.yaml` contract.

## Goals

- Split the crate into a Cargo workspace (`core`, `cli`, `server`) so CLI and server can share business logic via function calls (not shell-out)
- Define the shape of `.workdown/views.yaml`
- Validate `views.yaml` at load time using a JSON Schema, matching the existing pattern for `schema.yaml`

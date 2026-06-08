---
id: view-authoring
type: milestone
status: to_do
title: Author and edit views from the UI
parent: phase-04-visualization
depends_on: [server]
start_date: 2026-06-26
end_date: 2026-07-10
duration: "2w 1d"
---

Today views can only be created or changed by editing `views.yaml` by hand. The serve UI can render every view kind and navigate between them, but it cannot bring a new view into existence or adjust an existing one's filter. This milestone closes that gap: a user working in the browser can compose a new view and narrow any view's items without leaving the app — while `views.yaml` stays the source of truth and the user still commits on their own schedule.

## Outcomes

- A user can create a new view from the UI and have it appear in the navigation and render immediately.
- A user can adjust which items a view shows — both as a throwaway "for right now" narrowing and as a saved change written into `views.yaml`.
- The same filter-building experience is shared between "narrow this view" and "create a new view" — built once, reused.
- Anything written to `views.yaml` is validated the same way a hand-edited file would be, and surfaces the same diagnostics; nothing is committed automatically.

## Boundaries

- This is not a full `views.yaml` editor. Editing a view's every slot (renaming, changing kind, reordering columns, deleting) stays a text-editor job. The UI only needs to *create* a view and *adjust its filter*.
- Editing work items is a separate, already-covered concern — out of scope here.

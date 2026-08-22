# Mobile pull-with-rebase direction

## Product goal

Syntaxis should offer an opinionated **Pull with rebase** workflow that a user can
complete comfortably from a phone. It should solve the common case where a local
branch and its upstream have diverged without exposing Git's general interactive
rebase interface.

The intended flow is:

`Diverged → Pull with rebase → Resolve conflicts if needed → Continue → Synced`

The existing merge workflow remains available. Rebase is an additional explicit
choice, never an automatic replacement for merge and never selected without the
user's action.

## Mobile interaction model

Keep the workflow linear and focused on one decision at a time:

1. The diverged Git menu offers **Pull with rebase…** alongside **Merge
   upstream…**.
2. A short confirmation explains that local commits will be replayed on top of
   the upstream branch.
3. A clean rebase completes without further interaction.
4. If Git stops, the Git module enters a persistent rebase state showing progress
   such as **Rebasing commit 2 of 5** and the files that need attention.
5. The user resolves files through the existing conflict editor. Once every
   conflict for the current commit is resolved, one prominent **Continue rebase**
   action becomes available.
6. The cycle repeats until complete. **Abort rebase** remains accessible at every
   stopped step; **Skip commit** is a guarded secondary action in an overflow
   menu.

The UI should use a single-column file and conflict flow on narrow screens. Do
not introduce desktop-style three-pane layouts, a rebase todo editor, commit
reordering, or history-editing controls as part of this feature.

## Language and safety

Avoid Git's “ours” and “theirs” labels during a rebase because their meaning is
easy to misread. Prefer language such as **upstream version** and **rebased commit
version**, with the commit subject visible for context.

An in-progress rebase must survive navigation, refresh, and reconnect. The Git
module should always make the repository state and next valid action clear; it
must not present ordinary pull, push, merge, branch-switch, or commit actions
while they would interfere with that state.

Errors must preserve the repository's recoverable state. Unsupported cases such
as binary, submodule, or unusual rename conflicts should explain what happened,
offer **Abort rebase**, and provide a clear terminal fallback rather than
attempting an unsafe automatic resolution.

## Scope boundary

This feature includes starting a non-interactive pull-with-rebase operation,
detecting and presenting its progress, resolving supported conflicts, and
continuing, skipping, or aborting it.

Interactive rebase features—reordering, squashing, editing, or dropping an
arbitrary commit list—are deliberately out of scope. They require a different,
more complex interface and are not necessary to make routine divergence
resolution usable on mobile.

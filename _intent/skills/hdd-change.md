# HDD change skill

An agent skill that starts the process of changing intent.

**Goal**: help the user specify a change to the software by modifying the intent.

It follows these phases:

## P1. Establish change identity

Start by determining the nature of the change, either from the initial prompt or initial conversations with the user.

When ready:

1. pick the `<change-slug>`
2. create the `<change-dir>`
3. create the `<change-worktree>`

Tell the user the full paths to these directories so they can easily open them in an IDE.

## P2. Requirements and design

Work from the worktree you created or already exists.
Watch the `<change-worktree>/_intent` directory for changes and react to them.
Use a small debounce timeout ~30s to avoid flapping.

If the harness doesn't support watching, tell the user and look for changes when they respond.
Remind them they can say "c" or "continue" or something else short for you to loop.

Encourage the user to start a change by thinking about requirements.
They may or may not follow this, go with what they choose.
Requirements are optional in intent files, but they can be a very good place to start (and sometimes end).

The goal is to specify the change in a way an implementing agent won't find confusing or conflicting.

Do not encourage excessive detail.
Vague intent is fine if it is likely an implementing agent can build it reasonably.

Help the user:

1. Answer questions
2. Suggest changes
3. Check for mistakes and inconsistencies, broken links, etc

When changes are done and ready for implementation, you may help the user:

1. Commit changes
2. Push changes and create the PR; quote the user's intent as much as possible in the description

The user may solicit outside review before they implement, but that's up to them.

Offer to implement the changes when ready.
If you think useful, you may create a `<change-dir>/notes.ai.md`.
The implementing agent may use this.
Notes should not take the place of intent.
They serve as supplemental context drawn from your conversation with the user to steer the implementation in the direction you think the user most likely to want while not deviating from the intent.

Use a sub-agent with the [hdd-implement](./hdd-implement.md) skill to implement the changes.

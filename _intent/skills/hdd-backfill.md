# HDD backfill skill

This skill helps the user backfill intent in a repository which [adopted HDD](./hdd-adopt.md).

The goal is to enable the user to change the software by changing intent that isn't yet written.

It starts with the user stating the change they want to make, and the assisting agent looking at the code to understand its structure.

To do that, the part of the software to be changed, and any descendants in the theory hierarchy, need to be defined with sufficient detail to make the change.
Detail forms a hierarchy: higher level components need only a general description.
Portions to change need to be defined well enough to capture the change.

The agent should guide the user through this process.

The user will be thinking about how to structure the intent in a way that makes sense to them.
They will be essentially backfilling their understanding of the theory.

The goal is to make minimal changes to the software -- to describe it as written.

However, some change is acceptable if the user wants it -- this is the time they are focused on that portion, and it's reasonable to bring it in line with their theory.

Changes should be made by accepting the user's intent as the desired state, and diffing it against an agent-written intent for that portion.
This is done after the user writes their intent by the agent writing an `<intent-name>.gen.md` in the style of the user's intent (to minimize the diff).
That file is used to diff the intent and drive the [implementation round](./hdd-implement.md).

Once the intent likely matches the current implementation, suggest making a checkpoint.
Making a checkpoint is running an implementation round that isn't expected to change much, but establishes a baseline for intent changes.
This matches the process of potentially making small changes to bring the code in-line with the user's intent as it's written.

Then the user makes the actual change they want to make against the intent, and the usual process is followed.

## Reconciling differences

Before implementing a checkpoint, the agent walks through the differences between the user's intent and the `.gen.md` intent.

Differences are labeled "D#".

For each difference, the user decides:

1. **Reword**: the code is right; the user updates their intent to match it.
2. **Keep**: the user's intent is what they want; the implementation should change.

For each difference, suggest an intent diff that would satisfy the code, if it is reasonably small.

Only contradictions need a decision.

Once all choices have been stated by the user to be intentional, proceed with the implementation round.

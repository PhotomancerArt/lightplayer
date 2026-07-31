---
status: fixed
found: 2026-07-31      # how: prod, user-reported (agent debug export)
fixed: this change
area: lpa-agent (AgentSession::run_turn, OpenAI-compat provider)
class: trusted-upstream-self-report
---
# A tool call cut in half ran anyway, because the server said it wasn't cut

**Symptom** — A shader edit through the agent pane failed on prod with:

```
Agent error
stream ended before completion
Try sending again.
```

Retrying produced the same thing. The debug export showed the real first
failure one turn earlier — an `iterate` call rejected by its own tool
layer:

```
{"input_error":"invalid iterate input: invalid type: string
  \"{\\\"note\\\": \\\"Multiple hearts arranged radially…\",
  expected struct IterateInput"}
```

Nothing was staged (`"edits": []`), and the message named neither the
cause nor anything the model could act on.

**Root cause** — Three facts in the export line up. `output_tokens` was
exactly `8192` — the per-turn ceiling the compat provider used to send,
not a natural stop. The recorded tool input ended mid-string at
`vec2 pm = p - 0`, with no closing quote or brace: the model spent almost
the whole budget on reasoning (it derived the heart SDF by hand and wrote
the full shader out twice inside its thinking block) and hit the wall
partway through the `source` argument.

And the server reported `finish_reason: "tool_calls"`, not `"length"`.

`run_turn` already had a guard for exactly this case — it drops dangling
tool calls, and its comment names the scenario, "most often a `MaxTokens`
cut mid-input-JSON". But the guard keyed entirely off the mapped
`stop_reason`, so a server that mislabels its own cut walked straight
past it. The torn JSON then reached `Acc::to_content_block`, failed to
parse, and fell back to `Value::String(raw)` — a deliberate choice meant
to report malformed input in-band. Handed to `IterateInput`'s
deserializer, that produced a *type* error about a string where a struct
was expected, which is a true statement about the value and says nothing
about what actually happened.

The second failure — the one the user saw — is the EOF-without-
`finish_reason` path. The export only records the first attempt, so its
driver is not proven, but the compat dialect never replays thinking
blocks back to the model, so the retry re-derived the whole shader from
scratch into the same ceiling.

**Fix** — Two changes, both in `AgentSession`:

*Tool input that does not parse is now its own evidence of a cut.* If any
accumulated tool input fails to parse, the turn is reclassified as
`MaxTokens` regardless of what the server reported, which routes it into
the existing drop-and-report guard. The reclassification happens after
`TurnDone` is emitted, so the usage row still shows what the provider
actually said.

*The malformed-input placeholder is `Value::Null`, not the raw string.*
Handing a tool's deserializer the raw text is what converted a truncation
into a type mismatch; the block is dropped before execution now, so the
placeholder is never read.

No new user-facing copy was needed. `truncation_notice` already read
*"Run stopped: the response hit the output-token limit while writing the
edit — try again or ask for something smaller"* for this exact case — it
had simply never fired, because detection was the missing half.

**Also changed** — The compat provider no longer sends
`max_completion_tokens` at all (the constant `COMPAT_MAX_COMPLETION_TOKENS`
is gone). Any fixed number was a guess in both directions: 8k silently
truncates a turn that has to emit a whole shader, and a higher guess is
rejected or clamped by servers whose model max sits below it. Omitting
the field defers to each server's own model max, which is the number the
constant was trying to approximate. Consequence, recorded deliberately: a
runaway turn is now bounded by the run's turn limit rather than a token
ceiling. A per-connection override on `OpenAiCompatConfig` is the escape
hatch if that ever bites.

**Lesson** — The cap made truncation likely; the defect was executing a
call that had visibly not finished arriving. When a peer reports both a
*status* and the *data the status describes*, and the data can be checked,
the data outranks the status — a guard that only reads the status trusts
the one field the peer is most likely to get wrong. The tell here: the
guard's own comment described the failure correctly and still missed it,
because it was written against the honest case and tested only there.

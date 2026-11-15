# katnav: agent guide

A web browser in the terminal: Chromium renders off screen through CEF,
katui shows the pixels.

## Layout

- `src/main.rs`: arguments, the error enum, helper processes, wiring
- `src/engine.rs`: CEF: startup, the paint, display and life span
  handlers, input calls
- `src/tab.rs`: the katui `App`: pumps CEF, frames out, input in
- `src/governor.rs`: the render scale governor
- `src/keys.rs`: terminal keys as US keyboard physical keys

`just cef` downloads CEF into the data directory (`CEF_PATH` overrides
it), and `just bundle` builds the layout CEF needs: an app bundle with
helper apps on macOS, a folder on Linux. katnav runs from that layout
only.

katui comes from crates.io. To work against a local katui checkout, add a
`[patch.crates-io]` section to a `.cargo/config.toml` you keep untracked.

## Commands (the justfile is the single source of truth)

- `just check`: clippy with CI's denials; run after every change
- `just ci`: fmt check, clippy, tests, cargo deny, typos, complexity and
  duplication; must pass before any commit
- `just hooks`: install the prek git hooks, once per clone
- `just test`: unit tests
- `just bundle`: build katnav with CEF beside it
- `just run <url>`: bundle and open a page; `--mute` passes through
- Automated runs pass `--mute`, so no test plays sound on the host

## The loop

1. Keep the change small; one concern per commit.
2. `just check` after every change, `just ci` before committing.
3. Commit messages: one conventional line, `<type>: <lowercase imperative
   summary>` with types feat, fix, perf, refactor, test, docs, chore, ci,
   build, revert. No body.

## Rules

- No `unwrap`, `panic!` or `todo!` in non-test code; clippy denies them.
  `unsafe` is denied; the one exception reads CEF's paint buffer in
  `src/engine.rs`, with its safety comment.
- Errors: one `thiserror` enum. No `Result<_, String>` seams.
- Cognitive complexity over 20 fails clippy: the function grew a second
  job, split it.
- katnav stays a thin shell. Rendering, scripting and media belong to the
  browser; pixels, terminal input and presenters belong to katui. When
  katnav needs something katui lacks, add it to katui.
- Unit tests need no terminal and no CEF.
- Comments explain why, never what; default to zero. No change history.
- Prose in docs, commits and comments: declarative, terse, no em dashes.
- UI text is imperative and names the action the reader takes.
- Nothing personal in the repo: no real names, handles, emails, local
  machine paths or assistant attribution in code, docs, fixtures, commit
  messages or CI.
- `research/` is git-excluded local scratch. Tracked files never reference
  it or any local path.
- Do not weaken a gate to make it pass. Fix the code.

## Secrets

The repo holds none. Keep it that way.

# katnav: agent guide

A web browser in the terminal: headless Chrome renders, katui shows the
pixels.

## Layout

- `src/main.rs`: arguments, the error enum, wiring
- `src/chrome.rs`: launching the browser and the DevTools websocket thread
- `src/tab.rs`: the katui `App`: frames out, input in
- `src/keys.rs`: terminal keys as DOM keyboard events

katui comes from its public git repository. To work against a local katui
checkout, add a `[patch."https://github.com/h4ks-com/katui"]` section to a
`.cargo/config.toml` you keep untracked.

## Commands (the justfile is the single source of truth)

- `just check`: clippy with CI's denials; run after every change
- `just ci`: fmt check, clippy and tests; must pass before any commit
- `just test`: unit tests
- `just run <url>`: open a page

## The loop

1. Keep the change small; one concern per commit.
2. `just check` after every change, `just ci` before committing.
3. Commit messages: one conventional line, `<type>: <lowercase imperative
   summary>` with types feat, fix, perf, refactor, test, docs, chore, ci,
   build, revert. No body.

## Rules

- No `unwrap`, `panic!` or `todo!` in non-test code; clippy denies them.
  `unsafe` is forbidden.
- Errors: one `thiserror` enum. No `Result<_, String>` seams.
- Cognitive complexity over 20 fails clippy: the function grew a second
  job, split it.
- katnav stays a thin shell. Rendering, scripting and media belong to the
  browser; pixels, terminal input and presenters belong to katui. When
  katnav needs something katui lacks, add it to katui.
- Unit tests need no terminal and no browser.
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

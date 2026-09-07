---
name: xurl-unofficial-web-graphql
description: Use when maintaining xurl-unofficial X web GraphQL CLI.
version: 1.0.0
author: Hermes Agent
license: MIT
metadata:
  hermes:
    tags: [x, twitter, graphql, cookies, rust, impersonation]
    related_skills: [rustacean, github-pr-workflow]
---

# xurl-unofficial Web GraphQL Maintenance

## Overview

This repository builds an unofficial `xurl` binary that talks to X through the logged-in web client surface: browser cookies, web GraphQL operations, and selected web REST endpoints. It is not the paid X Developer API and must not be treated as OAuth `/2` infrastructure.

The hard boundary is simple:

- User identity is `auth_token` + `ct0` from browser cookies.
- The web Bearer token is public x.com client material, not a user secret.
- The CLI binary name is `xurl`; the crate/repo spelling is `xurl-unofficial`.
- Write paths may be wired but must not be live-regressed against a real personal account unless explicitly requested with concrete target/content.

## When to Use

Use this file when an agent is asked to maintain, review, extend, or debug this repository, especially:

- auth import from Chrome / Keychain behavior
- X web GraphQL operation catalogs
- `queryId` rotation and bundle scraping
- `x-client-transaction-id` generation
- Chrome TLS/HTTP2 impersonation
- search/timeline/bookmarks/followers/DM/media commands
- sensitive-data review before commits or pushes

Do not use it for the official `xurl` CLI that calls paid X API `/2` endpoints.

## Safety Rules

1. **Never print secrets.** Do not print `auth_token`, `ct0`, full `Cookie:` headers, `~/.xurl-unofficial/cookies.toml`, or browser cookie values.
2. **No accidental official config.** This repo stores cookies under `~/.xurl-unofficial/cookies.toml`, not `~/.xurl`.
3. **No real write regression by default.** Do not run post/reply/quote/delete/like/repost/follow/bookmark/block/mute/DM/media-upload against a real personal account unless the user explicitly asks for that exact side effect.
4. **Use public placeholders.** README/tests should use `alice`, `bob`, `x`, or other non-user handles. Do not bake personal handles into docs/tests/history.
5. **No streaming surface.** Do not add official `xurl -s`, web polling loops, WebSocket claims, or LivePipeline watchers unless the user reverses the prior decision.

## Architecture

- `src/auth.rs` — cookie file, env aliases, redacted debug, overwrite confirmation.
- `src/browser.rs` — imports `auth_token` and `ct0` from Chrome via `pookie`.
- `src/http.rs` — HTTP adapter, Chrome impersonation, web Bearer, cookies, transaction header, queryId refresh retry.
- `src/catalog.rs` + `catalog.json` — bundled GraphQL operation metadata.
- `src/bundle.rs` — live JS scraping for `operationName` → `queryId` refresh.
- `src/client.rs` — named domain methods over GraphQL/web REST.
- `src/parse.rs` — traversal/parsing for tweets, users, DMs, cursors.
- `src/search.rs` — official-ish search flags mapped to web search syntax.
- `src/media.rs` — media type/category and upload constraints.
- `src/types.rs` — boundary types and validation.
- `src/main.rs` — CLI routing only.

Keep IO in adapters. Keep parsing and validation pure where possible.

## Auth Flow

`xurl auth browser` is intentionally browser-first:

1. Resolve target cookie file.
2. If the file exists, prompt `Overwrite? [y/N]` before reading Keychain.
3. Open `https://x.com` so the user can log in or confirm the browser session.
4. Wait for Enter.
5. Import only `auth_token` and `ct0` from Chrome.
6. Save `~/.xurl-unofficial/cookies.toml` mode 0600.

If Chrome import fails, fall back to manual prompt. Values must never be echoed.

macOS may ask for the login Keychain / Chrome Safe Storage. That is system ACL behavior. Tell users to choose **Always Allow** if they want future imports to be quiet. Do not type passwords or interact with 1Password/2FA for them.

## HTTP / Anti-Bot Layers

Current layers:

| Layer | Status | Notes |
|---|---|---|
| `auth_token` + `ct0` | implemented | session identity and CSRF |
| web `WEB_BEARER` | implemented | public x.com client Bearer, not a user credential |
| Chrome TLS/HTTP2 impersonation | implemented | `wreq` + `wreq_util::Emulation::Chrome149` |
| `x-client-transaction-id` | best-effort | `x-client-transaction` parses live x.com/`ondemand.s` and generates per method/path |
| `queryId` refresh | best-effort | on GraphQL 404, scrape live JS and retry once |
| full browser cookie jar | not implemented | only `auth_token` + `ct0` are persisted |
| browser-controlled requests | not implemented | no CDP fetch replay or Chrome network stack |

`x-client-transaction-id` is not a static secret. It is generated per request from method + path + timestamp + page-derived animation key. If the parser fails because X changed the frontend, the current policy is to skip the header rather than fail every request.

`queryId` values rotate when X ships web bundles. `catalog.json` is only a fallback. A 404 on `/i/api/graphql/<queryId>/<operation>` should trigger live bundle scrape and exactly one retry.

## Chrome Impersonation Build Notes

`wreq` builds BoringSSL and bindgen. On Apple Silicon, bindgen may find an x86_64 Homebrew `libclang` first and fail. The repository includes `.cargo/config.toml` pointing `LIBCLANG_PATH` at the CommandLineTools universal libclang.

If build fails around `btls-sys` / `bindgen` / `libclang`:

```bash
find /Applications/Xcode.app /Library/Developer/CommandLineTools /opt/homebrew -name 'libclang.dylib' 2>/dev/null | while read p; do file "$p"; done
```

Pick an arm64/universal path and set `LIBCLANG_PATH` in `.cargo/config.toml`.

## QueryId Refresh Discipline

When adding a GraphQL operation:

1. Add bundled metadata to `catalog.json` with `queryId`, `method`, sample `variables`, and `features`.
2. Add a named client method in `src/client.rs`.
3. Add parse support if the payload shape is new.
4. Add a catalog assertion test for the operation.
5. Keep runtime refresh in memory only; do not write to `catalog.json` during command execution.

If a live command 404s after refresh, treat it as a catalog/method/feature mismatch, not automatically as auth failure. X can change the HTTP method as well as `queryId`.

## Verification Commands

Run the Rustacean gate before committing:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- \
  -D warnings \
  -D clippy::unwrap_used \
  -D clippy::expect_used \
  -D clippy::panic \
  -D clippy::todo \
  -D clippy::unimplemented \
  -D clippy::indexing_slicing
cargo test --all-targets --all-features
```

Safe live smoke tests:

```bash
cargo run -- auth status
cargo run -- user x
cargo run -- search "from:X" -n 1
```

Do not run write commands as smoke tests.

## Sensitive-Data Audit

Before commits and especially before force-push/history rewrite:

```bash
git status --ignored
git grep -n 'auth_token\|ct0\|Cookie:\|BEGIN .*PRIVATE\|api[_-]\?key\|secret\|password' || true
git log --all --full-history --name-only --pretty=format:
git grep -n '<personal-handle>' $(git rev-list --all) || true
```

Expected sensitive-file protections in `.gitignore` include `.env*`, `cookies.toml`, `*.cookies.toml`, `.xurl`, `.xurl-unofficial/`, private keys, and generic credential files.

## Common Pitfalls

1. **Treating `WEB_BEARER` as a user token.** It is public client material. Cookies carry identity.
2. **Assuming `queryId` is stable.** It rotates; method/features can also drift.
3. **Hard-coding transaction IDs.** They are per request and time-dependent.
4. **Holding a mutex across network await.** Do not keep catalog/tid locks while awaiting HTTP.
5. **Using `unwrap`/`expect` in production code.** The crate denies these clippy lints.
6. **Replacing `xurl auth browser` with headless browser auth.** The intended UX opens real Chrome; do not steal focus or handle passwords.
7. **Live-testing writes.** Wiring a write path is not permission to execute it.
8. **Adding personal handles to examples.** Use neutral examples and history-rewrite immediately if leaked.

## Completion Checklist

- [ ] CLI binary remains `xurl`.
- [ ] Cookie path remains `~/.xurl-unofficial/cookies.toml`.
- [ ] Secrets are not printed, committed, or included in messages.
- [ ] New operations have catalog entry, client method, parsing, and tests.
- [ ] `queryId` refresh remains best-effort and in-memory.
- [ ] `x-client-transaction-id` failure degrades safely.
- [ ] Chrome impersonation still builds on macOS with project libclang config.
- [ ] `cargo fmt --check`, clippy deny gate, and tests pass.
- [ ] Only safe read-only live smoke tests were run unless user explicitly authorized writes.

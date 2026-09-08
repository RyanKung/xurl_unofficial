# xurl-unofficial

The installed command is **`xurl`**. It talks to X **web GraphQL** with browser session cookies (`auth_token`, `ct0`, and when available the full browser `Cookie` header). It does **not** use the paid X Developer API.

This is not an official X client. GraphQL query IDs rotate; `catalog.json` is the current bundled map.

## Commands

```bash
xurl auth status
xurl whoami
xurl user alice
xurl read POST_ID
xurl search "from:alice" -n 10
xurl search "crypto lang:en -is:retweet min_likes:100" -n 10 --sort-order relevancy
xurl search "bitcoin lang:en" -n 10 --start-time 2026-09-01 --end-time 2026-09-07
xurl search "crypto lang:en" -n 10 --next-token TOKEN
xurl timeline -n 20
xurl post "Hello"
xurl reply POST_ID "Nice post"
xurl quote POST_ID "My take"
xurl quote POST_ID "$(cat long-note.txt)" # X Premium: >280 weighted chars uses CreateNoteTweet
xurl delete POST_ID
xurl like POST_ID
xurl unlike POST_ID
xurl repost POST_ID
xurl unrepost POST_ID
xurl follow @handle
xurl unfollow @handle
xurl mentions -n 10
xurl bookmarks -n 10
xurl likes -n 10
xurl following -n 20
xurl following --of alice -n 20
xurl followers -n 20
xurl bookmark POST_ID
xurl unbookmark POST_ID
xurl block @handle
xurl unblock @handle
xurl mute @handle
xurl unmute @handle
xurl dms -n 10
xurl dm @handle "hello"
xurl media upload photo.jpg
xurl media status MEDIA_ID
xurl post "Hello" --media-id MEDIA_ID
```

Stdout is JSON, same envelope as xurl: `{ "data": ... }` or `{ "errors": [...] }`.

## Auth

```bash
xurl auth
xurl auth login
xurl auth browser
xurl auth status
```

`xurl auth` keeps the easy manual path: it prompts for `auth_token`, then `ct0`, then an optional full `Cookie` header. Press Enter at the optional prompt to skip it. It writes `~/.xurl-unofficial/cookies.toml` (mode 0600). If that file already exists, it asks `Overwrite? [y/N]` before writing (and before Keychain, for `auth browser`). `N` or empty leaves the file unchanged. Auth commands print a short English status line, not JSON. Values are never printed.

`xurl auth browser` opens https://x.com, waits for Enter, then reads `auth_token`, `ct0`, and the full X/Twitter cookie header from Chrome. If that cookies file already exists, it asks `Overwrite? [y/N]` first. If macOS shows a Keychain dialog, choose **Always Allow**. If import fails, it falls back to the same prompts.

Write operations are more reliable with the full browser `Cookie` header. If only `auth_token` + `ct0` are configured, reads usually work but X may reject writes with code 226 (`looks like automated behavior`). Prefer `xurl auth browser` over manually copying a complete cookie string.

Do not paste cookies into chat.

Env aliases (same as polyoracle):

- `TWITTER_AUTH_TOKEN` / `TWITTER_COOKIE_AUTH_TOKEN`
- `TWITTER_CT0` / `TWITTER_COOKIE_CT0`
- `TWITTER_COOKIE_HEADER` / `X_COOKIE_HEADER` for a full browser Cookie header
- `XURL_COOKIES_FILE` for an explicit toml path

## Model incompleteness

- **WEB_BEARER** in `src/http.rs` is the public x.com web-client token, not a user cookie. Identity is browser cookies; writes should use the full browser Cookie header when available. Do not store it in `~/.xurl`.
- **Chrome impersonate.** Requests use `wreq` Chrome TLS/HTTP2 emulation, not rustls `reqwest`.
- **query IDs expire** when X ships a new web bundle. GraphQL 404 on a named operation triggers a live JS scrape and one retry. `catalog.json` is only the bundled fallback.
- **Long posts use `CreateNoteTweet`.** Text over 280 weighted chars routes to the separate NoteTweet mutation with its own feature flags and `fieldToggles`; URLs count as 23 chars.
- **`x-client-transaction-id`** is generated from the live x.com homepage + `ondemand.s` when that parser still matches. If X changes the animation/JS contract, the header is skipped rather than failing every call.
- **Writes are wired only.** `post` / `reply` / `quote` / `delete` / `like` / `repost` / `follow` / `bookmark` / `block` / `mute` / `dm` / `media upload` have not been live-regressed against a personal account.

Write actions use undocumented web mutations. They can 404 when X rotates query IDs.

//! CLI entry for `xurl`.

#![deny(missing_docs)]

use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::json;
use xurl_unofficial::auth::{confirms_overwrite, AuthSaved};
use xurl_unofficial::browser;
use xurl_unofficial::error::{AuthField, Error};
use xurl_unofficial::{
    parse_media_ids, AuthOrigin, CookiesFile, MediaId, PageSize, PostId, PostText, ScreenName,
    SessionCookies, SortOrder, XClient,
};

#[derive(Parser)]
#[command(
    name = "xurl",
    about = "xurl-compatible CLI using X web GraphQL cookies"
)]
struct Cli {
    /// Path to cookies.toml (auth_token, ct0).
    #[arg(long, env = "XURL_COOKIES_FILE")]
    cookies_file: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the authenticated user.
    Whoami,
    /// Look up a user by handle.
    User {
        /// Handle, with or without `@`.
        handle: String,
    },
    /// Read a post by id or status URL.
    Read {
        /// Numeric id or https://x.com/.../status/ID.
        post_id: String,
    },
    /// Create a post.
    Post {
        /// Post body.
        text: String,
        /// Official `--media-id` (repeatable, max 4).
        #[arg(long = "media-id")]
        media_id: Vec<String>,
    },
    /// Reply to a post.
    Reply {
        /// Parent post id or URL.
        post_id: String,
        /// Reply body.
        text: String,
        /// Official `--media-id` (repeatable, max 4).
        #[arg(long = "media-id")]
        media_id: Vec<String>,
    },
    /// Quote a post.
    Quote {
        /// Quoted post id or URL.
        post_id: String,
        /// Quote body.
        text: String,
        /// Official `--media-id` (repeatable, max 4).
        #[arg(long = "media-id")]
        media_id: Vec<String>,
    },
    /// Delete a post owned by this session.
    Delete {
        /// Post id or URL.
        post_id: String,
    },
    /// Search posts.
    Search {
        /// Official `query` string. API operators are rewritten for web search.
        query: String,
        /// Official `max_results` (`-n`), 1..=100.
        #[arg(short = 'n', long, default_value_t = 10)]
        count: u32,
        /// Official `sort_order`: recency (Latest) or relevancy (Top).
        #[arg(long, default_value = "recency")]
        sort_order: String,
        /// Official `start_time` (YYYY-MM-DD or RFC3339) → `since:`.
        #[arg(long)]
        start_time: Option<String>,
        /// Official `end_time` (YYYY-MM-DD or RFC3339) → `until:`.
        #[arg(long)]
        end_time: Option<String>,
        /// Official `next_token`.
        #[arg(long)]
        next_token: Option<String>,
        /// Official `pagination_token` (alias of `next_token`).
        #[arg(long)]
        pagination_token: Option<String>,
    },
    /// Home latest timeline.
    Timeline {
        /// Max results, 1..=100.
        #[arg(short = 'n', long, default_value_t = 20)]
        count: u32,
        /// Official `next_token`.
        #[arg(long)]
        next_token: Option<String>,
        /// Official `pagination_token` (alias of `next_token`).
        #[arg(long)]
        pagination_token: Option<String>,
    },
    /// Favorite a post.
    Like {
        /// Post id or URL.
        post_id: String,
    },
    /// Remove a favorite.
    Unlike {
        /// Post id or URL.
        post_id: String,
    },
    /// Repost a post.
    Repost {
        /// Post id or URL.
        post_id: String,
    },
    /// Undo a repost.
    Unrepost {
        /// Post id or URL.
        post_id: String,
    },
    /// Follow a user.
    Follow {
        /// Handle, with or without `@`.
        handle: String,
    },
    /// Unfollow a user.
    Unfollow {
        /// Handle, with or without `@`.
        handle: String,
    },
    /// Mentions.
    Mentions {
        /// Max results, 1..=100.
        #[arg(short = 'n', long, default_value_t = 10)]
        count: u32,
        /// Official `next_token`.
        #[arg(long)]
        next_token: Option<String>,
        /// Official `pagination_token`.
        #[arg(long)]
        pagination_token: Option<String>,
    },
    /// Bookmarked posts.
    Bookmarks {
        /// Max results, 1..=100.
        #[arg(short = 'n', long, default_value_t = 10)]
        count: u32,
        /// Official `next_token`.
        #[arg(long)]
        next_token: Option<String>,
        /// Official `pagination_token`.
        #[arg(long)]
        pagination_token: Option<String>,
    },
    /// Liked posts.
    Likes {
        /// Another user's likes.
        #[arg(long)]
        of: Option<String>,
        /// Max results, 1..=100.
        #[arg(short = 'n', long, default_value_t = 10)]
        count: u32,
        /// Official `next_token`.
        #[arg(long)]
        next_token: Option<String>,
        /// Official `pagination_token`.
        #[arg(long)]
        pagination_token: Option<String>,
    },
    /// Following list.
    Following {
        /// Another user.
        #[arg(long)]
        of: Option<String>,
        /// Max results, 1..=100.
        #[arg(short = 'n', long, default_value_t = 20)]
        count: u32,
        /// Official `next_token`.
        #[arg(long)]
        next_token: Option<String>,
        /// Official `pagination_token`.
        #[arg(long)]
        pagination_token: Option<String>,
    },
    /// Followers list.
    Followers {
        /// Another user.
        #[arg(long)]
        of: Option<String>,
        /// Max results, 1..=100.
        #[arg(short = 'n', long, default_value_t = 20)]
        count: u32,
        /// Official `next_token`.
        #[arg(long)]
        next_token: Option<String>,
        /// Official `pagination_token`.
        #[arg(long)]
        pagination_token: Option<String>,
    },
    /// Bookmark a post.
    Bookmark {
        /// Post id or URL.
        post_id: String,
    },
    /// Remove a bookmark.
    Unbookmark {
        /// Post id or URL.
        post_id: String,
    },
    /// Block a user.
    Block {
        /// Handle, with or without `@`.
        handle: String,
    },
    /// Unblock a user.
    Unblock {
        /// Handle, with or without `@`.
        handle: String,
    },
    /// Mute a user.
    Mute {
        /// Handle, with or without `@`.
        handle: String,
    },
    /// Unmute a user.
    Unmute {
        /// Handle, with or without `@`.
        handle: String,
    },
    /// Inbox messages.
    Dms {
        /// Max results, 1..=100.
        #[arg(short = 'n', long, default_value_t = 10)]
        count: u32,
    },
    /// Send a direct message.
    Dm {
        /// Handle, with or without `@`.
        handle: String,
        /// Message body.
        text: String,
    },
    /// Media upload helpers.
    Media {
        #[command(subcommand)]
        action: MediaCommand,
    },
    /// Configure session cookies.
    Auth {
        #[command(subcommand)]
        action: Option<AuthCommand>,
    },
}

#[derive(Subcommand)]
enum AuthCommand {
    /// Prompt for auth_token and ct0, then save.
    Login,
    /// Report whether cookies are configured, without printing them.
    Status,
    /// Open x.com and import auth_token and ct0 from Chrome.
    Browser,
}

#[derive(Subcommand)]
enum MediaCommand {
    /// INIT/APPEND/FINALIZE a local file. Does not create a post.
    Upload {
        /// Image or mp4 path.
        path: PathBuf,
    },
    /// Poll processing for an uploaded media id.
    Status {
        /// Numeric media id.
        media_id: String,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let _ = print_err(&err);
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Error> {
    let cli = Cli::parse();
    let dest = cli.cookies_file.map(CookiesFile::new);
    match cli.command {
        Command::Auth { action } => run_auth(action, dest),
        command => {
            let session = SessionCookies::load(dest.as_ref())?;
            let client = XClient::new(session)?;
            dispatch(&client, command).await
        }
    }
}

fn run_auth(action: Option<AuthCommand>, dest: Option<CookiesFile>) -> Result<(), Error> {
    match action {
        None | Some(AuthCommand::Login) => save_from_prompt(dest),
        Some(AuthCommand::Status) => print_line(&SessionCookies::status(dest.as_ref()).to_string()),
        Some(AuthCommand::Browser) => save_from_browser(dest),
    }
}

fn save_from_prompt(dest: Option<CookiesFile>) -> Result<(), Error> {
    require_tty()?;
    let session = prompt_session()?;
    persist(session, dest, AuthOrigin::Prompt)
}

fn save_from_browser(dest: Option<CookiesFile>) -> Result<(), Error> {
    require_tty()?;
    let file = CookiesFile::or_default(dest)?;
    confirm_overwrite(&file)?;
    open::that("https://x.com").map_err(|err| Error::BrowserOpen(err.to_string()))?;
    writeln!(
        io::stderr(),
        "Log in to X in Chrome if needed, then press Enter to import cookies.\nIf macOS asks for the login keychain, enter it once and choose Always Allow."
    )
    .map_err(|source| Error::Io { path: None, source })?;
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|source| Error::Io { path: None, source })?;
    match browser::session_from_chrome() {
        Ok(session) => write_session(session, &file, AuthOrigin::Chrome),
        Err(err) => {
            writeln!(io::stderr(), "{err}\nEnter auth_token and ct0 manually.")
                .map_err(|source| Error::Io { path: None, source })?;
            let session = prompt_session()?;
            write_session(session, &file, AuthOrigin::Prompt)
        }
    }
}

fn persist(
    session: SessionCookies,
    dest: Option<CookiesFile>,
    origin: AuthOrigin,
) -> Result<(), Error> {
    let file = CookiesFile::or_default(dest)?;
    confirm_overwrite(&file)?;
    write_session(session, &file, origin)
}

fn write_session(
    session: SessionCookies,
    file: &CookiesFile,
    origin: AuthOrigin,
) -> Result<(), Error> {
    session.save(file)?;
    print_line(&AuthSaved::new(file, origin).to_string())
}

fn confirm_overwrite(file: &CookiesFile) -> Result<(), Error> {
    if !file.exists() {
        return Ok(());
    }
    writeln!(
        io::stderr(),
        "Cookies already exist at {}. Overwrite? [y/N]",
        file.as_path().display()
    )
    .map_err(|source| Error::Io { path: None, source })?;
    let answer = prompt_line("overwrite")?;
    if confirms_overwrite(&answer) {
        Ok(())
    } else {
        Err(Error::AuthCancelled)
    }
}

fn prompt_session() -> Result<SessionCookies, Error> {
    let token = prompt_line("auth_token")?;
    if token.is_empty() {
        return Err(Error::MissingAuth(AuthField::AuthToken));
    }
    let ct0 = prompt_line("ct0")?;
    if ct0.is_empty() {
        return Err(Error::MissingAuth(AuthField::Ct0));
    }
    writeln!(
        io::stderr(),
        "Optional for write reliability: paste the full Cookie header, or press Enter to skip."
    )
    .map_err(|source| Error::Io { path: None, source })?;
    let cookie_header = prompt_line("cookie_header")?;
    let cookie_header = if cookie_header.is_empty() {
        None
    } else {
        Some(cookie_header)
    };
    SessionCookies::with_cookie_header(token, ct0, cookie_header)
}

fn prompt_line(label: &str) -> Result<String, Error> {
    write!(io::stderr(), "{label}: ").map_err(|source| Error::Io { path: None, source })?;
    io::stderr()
        .flush()
        .map_err(|source| Error::Io { path: None, source })?;
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|source| Error::Io { path: None, source })?;
    Ok(line.trim().to_string())
}

fn require_tty() -> Result<(), Error> {
    if io::stdin().is_terminal() {
        Ok(())
    } else {
        Err(Error::AuthNotInteractive)
    }
}

async fn dispatch(client: &XClient, command: Command) -> Result<(), Error> {
    match command {
        Command::Whoami => print_json(&client.whoami().await?),
        Command::User { handle } => {
            let name = ScreenName::parse(&handle)?;
            print_json(&client.user(&name).await?)
        }
        Command::Read { post_id } => {
            let id = PostId::parse(&post_id)?;
            print_json(&client.read(&id).await?)
        }
        Command::Post { text, media_id } => {
            let text = PostText::parse(&text)?;
            let media = parse_media_ids(&media_id)?;
            print_json(&client.post(&text, &media).await?)
        }
        Command::Reply {
            post_id,
            text,
            media_id,
        } => {
            let id = PostId::parse(&post_id)?;
            let text = PostText::parse(&text)?;
            let media = parse_media_ids(&media_id)?;
            print_json(&client.reply(&id, &text, &media).await?)
        }
        Command::Quote {
            post_id,
            text,
            media_id,
        } => {
            let id = PostId::parse(&post_id)?;
            let text = PostText::parse(&text)?;
            let media = parse_media_ids(&media_id)?;
            print_json(&client.quote(&id, &text, &media).await?)
        }
        Command::Delete { post_id } => {
            let id = PostId::parse(&post_id)?;
            client.delete(&id).await?;
            print_json(&json!({ "deleted": true, "id": id.as_str() }))
        }
        Command::Search {
            query,
            count,
            sort_order,
            start_time,
            end_time,
            next_token,
            pagination_token,
        } => {
            let query = xurl_unofficial::search::compile_query(
                &query,
                start_time.as_deref(),
                end_time.as_deref(),
            )?;
            let limit = PageSize::parse(count)?;
            let sort = SortOrder::parse(&sort_order)?;
            let cursor = xurl_unofficial::search::pick_pagination_token(
                next_token.as_deref(),
                pagination_token.as_deref(),
            )?;
            let (tweets, next) = client
                .search(&query, limit, sort, cursor.as_deref())
                .await?;
            print_page(&tweets, next.as_deref())
        }
        Command::Timeline {
            count,
            next_token,
            pagination_token,
        } => {
            let limit = PageSize::parse(count)?;
            let cursor = xurl_unofficial::search::pick_pagination_token(
                next_token.as_deref(),
                pagination_token.as_deref(),
            )?;
            let (tweets, next) = client.timeline(limit, cursor.as_deref()).await?;
            print_page(&tweets, next.as_deref())
        }
        Command::Like { post_id } => {
            let id = PostId::parse(&post_id)?;
            client.like(&id).await?;
            print_json(&json!({ "liked": true, "id": id.as_str() }))
        }
        Command::Unlike { post_id } => {
            let id = PostId::parse(&post_id)?;
            client.unlike(&id).await?;
            print_json(&json!({ "unliked": true, "id": id.as_str() }))
        }
        Command::Repost { post_id } => {
            let id = PostId::parse(&post_id)?;
            client.repost(&id).await?;
            print_json(&json!({ "reposted": true, "id": id.as_str() }))
        }
        Command::Unrepost { post_id } => {
            let id = PostId::parse(&post_id)?;
            client.unrepost(&id).await?;
            print_json(&json!({ "unreposted": true, "id": id.as_str() }))
        }
        Command::Follow { handle } => {
            let name = ScreenName::parse(&handle)?;
            let user = client.follow(&name).await?;
            print_json(&json!({ "following": true, "id": user.id, "username": user.username }))
        }
        Command::Unfollow { handle } => {
            let name = ScreenName::parse(&handle)?;
            let user = client.unfollow(&name).await?;
            print_json(&json!({ "following": false, "id": user.id, "username": user.username }))
        }
        Command::Mentions {
            count,
            next_token,
            pagination_token,
        } => {
            let limit = PageSize::parse(count)?;
            let cursor = list_cursor(next_token.as_deref(), pagination_token.as_deref())?;
            let (tweets, next) = client.mentions(limit, cursor.as_deref()).await?;
            print_page(&tweets, next.as_deref())
        }
        Command::Bookmarks {
            count,
            next_token,
            pagination_token,
        } => {
            let limit = PageSize::parse(count)?;
            let cursor = list_cursor(next_token.as_deref(), pagination_token.as_deref())?;
            let (tweets, next) = client.bookmarks(limit, cursor.as_deref()).await?;
            print_page(&tweets, next.as_deref())
        }
        Command::Likes {
            of,
            count,
            next_token,
            pagination_token,
        } => {
            let limit = PageSize::parse(count)?;
            let of = parse_of(of.as_deref())?;
            let cursor = list_cursor(next_token.as_deref(), pagination_token.as_deref())?;
            let (tweets, next) = client.likes(of.as_ref(), limit, cursor.as_deref()).await?;
            print_page(&tweets, next.as_deref())
        }
        Command::Following {
            of,
            count,
            next_token,
            pagination_token,
        } => {
            let limit = PageSize::parse(count)?;
            let of = parse_of(of.as_deref())?;
            let cursor = list_cursor(next_token.as_deref(), pagination_token.as_deref())?;
            let (users, next) = client
                .following(of.as_ref(), limit, cursor.as_deref())
                .await?;
            print_page(&users, next.as_deref())
        }
        Command::Followers {
            of,
            count,
            next_token,
            pagination_token,
        } => {
            let limit = PageSize::parse(count)?;
            let of = parse_of(of.as_deref())?;
            let cursor = list_cursor(next_token.as_deref(), pagination_token.as_deref())?;
            let (users, next) = client
                .followers(of.as_ref(), limit, cursor.as_deref())
                .await?;
            print_page(&users, next.as_deref())
        }
        Command::Bookmark { post_id } => {
            let id = PostId::parse(&post_id)?;
            client.bookmark(&id).await?;
            print_json(&json!({ "bookmarked": true, "id": id.as_str() }))
        }
        Command::Unbookmark { post_id } => {
            let id = PostId::parse(&post_id)?;
            client.unbookmark(&id).await?;
            print_json(&json!({ "bookmarked": false, "id": id.as_str() }))
        }
        Command::Block { handle } => {
            let name = ScreenName::parse(&handle)?;
            let user = client.block(&name).await?;
            print_json(&json!({ "blocked": true, "id": user.id, "username": user.username }))
        }
        Command::Unblock { handle } => {
            let name = ScreenName::parse(&handle)?;
            let user = client.unblock(&name).await?;
            print_json(&json!({ "blocked": false, "id": user.id, "username": user.username }))
        }
        Command::Mute { handle } => {
            let name = ScreenName::parse(&handle)?;
            let user = client.mute(&name).await?;
            print_json(&json!({ "muted": true, "id": user.id, "username": user.username }))
        }
        Command::Unmute { handle } => {
            let name = ScreenName::parse(&handle)?;
            let user = client.unmute(&name).await?;
            print_json(&json!({ "muted": false, "id": user.id, "username": user.username }))
        }
        Command::Dms { count } => {
            let limit = PageSize::parse(count)?;
            print_json(&client.dms(limit).await?)
        }
        Command::Dm { handle, text } => {
            let name = ScreenName::parse(&handle)?;
            let text = PostText::parse(&text)?;
            print_json(&client.dm(&name, &text).await?)
        }
        Command::Media { action } => match action {
            MediaCommand::Upload { path } => print_json(&client.upload_media(&path).await?),
            MediaCommand::Status { media_id } => {
                let id = MediaId::parse(&media_id)?;
                print_json(&client.media_status(&id).await?)
            }
        },
        Command::Auth { .. } => print_line(&SessionCookies::status(None).to_string()),
    }
}

fn print_line(text: &str) -> Result<(), Error> {
    writeln!(io::stdout(), "{text}").map_err(|source| Error::Io { path: None, source })
}

fn list_cursor(
    next_token: Option<&str>,
    pagination_token: Option<&str>,
) -> Result<Option<String>, Error> {
    xurl_unofficial::search::pick_pagination_token(next_token, pagination_token)
}

fn parse_of(of: Option<&str>) -> Result<Option<ScreenName>, Error> {
    match of {
        Some(raw) => Ok(Some(ScreenName::parse(raw)?)),
        None => Ok(None),
    }
}

fn print_json<T: Serialize>(data: &T) -> Result<(), Error> {
    let payload = json!({ "data": data });
    write_json(&payload)
}

fn print_page<T: Serialize>(tweets: &[T], next_token: Option<&str>) -> Result<(), Error> {
    let mut meta = json!({ "result_count": tweets.len() });
    if let Some(token) = next_token {
        if let Some(object) = meta.as_object_mut() {
            object.insert("next_token".to_string(), json!(token));
        }
    }
    write_json(&json!({ "data": tweets, "meta": meta }))
}

fn write_json(payload: &serde_json::Value) -> Result<(), Error> {
    let encoded = serde_json::to_string_pretty(payload)?;
    writeln!(io::stdout(), "{encoded}").map_err(|source| Error::Io { path: None, source })
}

fn print_err(err: &Error) -> io::Result<()> {
    let payload = json!({
        "errors": [{ "message": err.to_string() }]
    });
    match serde_json::to_string_pretty(&payload) {
        Ok(encoded) => writeln!(io::stderr(), "{encoded}"),
        Err(_) => writeln!(io::stderr(), "{err}"),
    }
}

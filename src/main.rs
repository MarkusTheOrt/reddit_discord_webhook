use std::{
    borrow::Cow,
    time::{Duration, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose};
use libsql::params;
use reqwest::{ClientBuilder, header::HeaderMap};
use sentry::{IntoDsn, TransactionContext, protocol::SpanStatus};
use serde::Serialize;
use tracing::{info, warn};

use crate::model::*;

mod model;

const USER_AGENT: &str = concat!(
    "formula1discordredditapp:markus-dev@",
    env!("CARGO_PKG_VERSION")
);

const REDDIT_LOGO: &str = "https://fia.ort.dev/reddit_logo.png";

const BANNED_URLS: [&str; 3] = ["reddit.com", "redd.it", "f1-insider.com"];

const MIN_UPVOTES: i32 = 100;

#[derive(Serialize)]
pub struct Author<'a> {
    name: Cow<'a, str>,
    icon_url: Cow<'a, str>,
}

#[derive(Serialize)]
pub struct Image<'a> {
    url: &'a str,
}

#[derive(Serialize)]
pub struct Embed<'a> {
    title: Option<Cow<'a, str>>,
    description: Option<&'a str>,
    image: Option<Image<'a>>,
    color: u32,
    url: Option<Cow<'a, str>>,
    author: Author<'a>,
}

#[derive(Serialize)]
pub struct WebhookMessage<'a> {
    content: &'a str,
    embeds: Vec<Embed<'a>>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().init();

    _ = dotenvy::dotenv();

    let c = if let Ok(var) = std::env::var("SENTRY_DSN") {
        Some(sentry::init(sentry::ClientOptions {
            dsn: var.into_dsn().expect("VALID SENTRY DSN"),
            traces_sample_rate: 1.0,
            sample_rate: 1.0,
            ..Default::default()
        }))
    } else {
        None
    };

    let webhook_url = std::env::var("WEBHOOK_URL").expect("Webhook URL not set!");
    let client_id = std::env::var("CLIENT_ID").expect("Client ID not set!");
    let secret = std::env::var("CLIENT_SECRET").expect("Secret key not set!");

    let creds = std::env::var("CREDENTIALS").expect("CREDS NOT SET");

    let _encoded_creds = general_purpose::STANDARD.encode(format!("{client_id}:{secret}"));

    let mut headers = HeaderMap::new();
    headers.append(
        "Authorization",
        format!("BASIC {creds}").parse().expect("Header invalid"),
    );

    let client = ClientBuilder::new()
        .user_agent(USER_AGENT)
        .default_headers(headers)
        .build()?;

    let database = libsql::Builder::new_local("database/db").build().await?;

    info!("Reddit webhook starting...");

    let mut posted_cache: Vec<Cow<str>> = Vec::with_capacity(100);
    let mut first_start = true;

    let db_conn = database.connect()?;

    let (tx, should_stop) = tokio::sync::watch::channel(());
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        tx.send(()).unwrap();
        info!("Shutting down!");
    });

    loop {
        if should_stop.borrow().has_changed() {
            break;
        }
        let tx = sentry::start_transaction(TransactionContext::new("Main Loop", "app.loop"));
        let span = tx.start_child("http.client", "Requesting Posts");
        span.set_request(sentry::protocol::Request {
            url: Some("https://www.reddit.com/search.json".parse().unwrap()),
            method: Some("GET".into()),
            query_string: Some(
                "?q=subreddit%3Aformula1%20flair%3Apost-news&source=recent&sort=hot&limit=100"
                    .into(),
            ),
            ..Default::default()
        });

        let req = client.get("https://www.reddit.com/search.json?q=subreddit%3Aformula1%20flair%3Apost-news&source=recent&sort=hot&limit=100")
        .send().await;
        let request = match req {
            Ok(data) => data,
            Err(why) => {
                sentry::capture_error(&why);
                span.set_status(sentry::protocol::SpanStatus::UnknownError);
                span.finish();
                tx.set_status(sentry::protocol::SpanStatus::Cancelled);
                tx.finish();
                tokio::time::sleep(Duration::from_secs(60)).await;
                continue;
            }
        };
        span.set_tag("http.status_code", request.status().as_u16());

        if let Err(why) = request.error_for_status_ref() {
            sentry::capture_error(&why);
            span.finish();
            tx.set_status(sentry::protocol::SpanStatus::Cancelled);
            tx.finish();
            tokio::time::sleep(Duration::from_secs(60)).await;
            continue;
        }

        span.set_status(sentry::protocol::SpanStatus::Ok);
        let data = request.json::<ReturnData>().await;
        let data = match data {
            Ok(data) => data,
            Err(why) => {
                sentry::capture_error(&why);
                span.finish();
                tx.set_status(sentry::protocol::SpanStatus::Cancelled);
                tx.finish();
                tokio::time::sleep(Duration::from_secs(60)).await;
                continue;
            }
        };
        span.finish();

        let nau = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Clock to be working")
            .as_secs();

        for child in data.data.children {
            let child = match child {
                ApiListing::T3(t3) => t3,
                _ => {
                    warn!("unsupported T-Type found!");
                    continue;
                }
            };

            // skip posts older than once week.
            if nau - child.created() > 60 * 60 * 24 * 7 {
                info!(
                    "Skipping {} due to age ({})",
                    child.id,
                    nau - child.created()
                );
                continue;
            }

            let mut is_banned = false;

            // skip over already posted shit.
            if posted_cache.contains(&child.id) || child.ups < MIN_UPVOTES || child.url.is_none() {
                continue;
            }

            let url = child.url.as_ref().unwrap();
            for banned in BANNED_URLS {
                if url.contains(banned) {
                    is_banned = true;
                    break;
                }
            }

            // we skip over banned websites as well, who wants that shit?
            if is_banned {
                continue;
            }
            let span = tx.start_child("db", "INSERT INTO reddit_posts (reddit_id) VALUES (?)");
            let res = db_conn
                .execute(
                    "INSERT INTO reddit_posts (reddit_id) VALUES (?)",
                    params![child.id],
                )
                .await;
            if let Err(why) = res {
                if let libsql::Error::SqliteFailure(a, _) = why {
                    if a == libsql::ffi::ErrorCode::ConstraintViolation as i32 {
                        span.set_status(SpanStatus::Ok);
                        posted_cache.push(child.id.clone());
                    }
                } else {
                    sentry::capture_error(&why);
                    span.set_status(SpanStatus::InternalError);
                    span.finish();
                    continue;
                }
            }
            span.set_status(SpanStatus::Ok);
            span.finish();

            let preview_url = format!("https://share.redd.it/preview/post/{}", child.id);
            let author_url = format!("u/{} on r/formula1", child.author);
            let reddit_url = format!("https://reddit.com{}", child.permalink);
            let mesasge = format!(
                "[go to Article on {}](<{}>)\n[go to Reddit post](<{}>)",
                child.domain, url, reddit_url
            );
            let message = WebhookMessage {
                content: "",
                embeds: vec![Embed {
                    title: Some(child.title),
                    color: 0xFF4500,
                    description: Some(&mesasge),
                    image: Some(Image { url: &preview_url }),
                    url: Some(child.url.unwrap()),
                    author: Author {
                        name: Cow::Borrowed(&author_url),
                        icon_url: Cow::Borrowed(REDDIT_LOGO),
                    },
                }],
            };
            if !first_start {
                let span = tx.start_child("http.client", "Sending Webhook");
                span.set_status(SpanStatus::Ok);
                span.set_request(sentry::protocol::Request {
                    url: Some(webhook_url.parse().unwrap()),
                    method: Some("POST".into()),
                    data: serde_json::to_string_pretty(&message).ok(),
                    ..Default::default()
                });
                let send = client.post(&webhook_url).json(&message).send().await;
                if let Err(why) = send {
                    sentry::capture_error(&why);
                    span.set_status(SpanStatus::Aborted);
                    why.status()
                        .inspect(|f| span.set_tag("http.status_code", f.as_u16()));
                }
                span.set_tag("http.status_code", 200);
                span.finish();
            }
        }
        if first_start {
            first_start = false;
        }
        tx.set_status(sentry::protocol::SpanStatus::Ok);
        tx.finish();
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
    drop(c);
    Ok(())
}

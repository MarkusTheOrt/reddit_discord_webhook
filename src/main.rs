use std::{
    borrow::Cow,
    time::{Duration, UNIX_EPOCH},
};

use base64::{engine::general_purpose, Engine as _};
use reqwest::{header::HeaderMap, ClientBuilder};
use serde::Serialize;
use sqlx::PgPool;
use tracing::{error, info, warn};

use crate::model::*;

mod model;

const USER_AGENT: &str = "formula1discordredditapp:markus-dev@v0.3.0";

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
async fn main() -> Result<(), ()> {
    tracing_subscriber::fmt().init();

    _ = dotenvy::dotenv();
    let webhook_url =
        std::env::var("WEBHOOK_URL").expect("Webhook URL not set!");
    let client_id = std::env::var("CLIENT_ID").expect("Client ID not set!");
    let secret = std::env::var("CLIENT_SECRET").expect("Secret key not set!");
    let database_url =
        std::env::var("DATABASE_URL").expect("Database URL not set!");
    
    let creds = std::env::var("CREDENTIALS").expect("CREDS NOT SET");

    let _encoded_creds =
        general_purpose::STANDARD.encode(format!("{client_id}:{secret}"));

    let mut headers = HeaderMap::new();
    headers.append(
        "Authorization",
        format!("BASIC {creds}").parse().expect("Header invalid"),
    );

    let client = ClientBuilder::new()
        .user_agent(USER_AGENT)
        .default_headers(headers)
        .build()
        .expect("clientbuilder");

    let database =
        PgPool::connect(&database_url).await.expect("Database Connection");

    let mut posted_cache: Vec<Cow<str>> = Vec::with_capacity(100);
    let mut first_start = true;
    loop {
        let test = client.get("https://www.reddit.com/search.json?q=subreddit%3Aformula1%20flair%3Apost-news&source=recent&sort=hot&limit=100")
        .send().await;
        println!("Looping!");
        let request = match test {
            Ok(data) => data,
            Err(why) => {
                error!("Error: {why}");
                std::thread::sleep(Duration::from_secs(60));
                continue;
            },
        };
        info!("looping louie!");
        if let Err(why) = request.error_for_status_ref() {
            error!("Error: {why}");
            std::thread::sleep(Duration::from_secs(60));
            continue;
        }

        let data = request.json::<ReturnData>().await;
        let data = match data {
            Ok(data) => data,
            Err(why) => {
                error!("Error decoding data: {why}");
                std::thread::sleep(Duration::from_secs(60));
                continue;
            },
        };

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
                },
            };

            // skip posts older than once week.
            if nau - child.created_utc > 60 * 60 * 24 * 7 {
                info!(
                    "Skipping {} due to age ({})",
                    child.id,
                    nau - child.created_utc
                );
                continue;
            }

            let mut is_banned = false;

            // skip over already posted shit.
            if posted_cache.contains(&child.id)
                || child.ups < MIN_UPVOTES
                || child.url.is_none()
            {
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

            let db_res =
                sqlx::query!("INSERT INTO reddit (id) VALUES ($1)", &child.id)
                    .execute(&database)
                    .await;
            if let Err(why) = db_res {
                if let sqlx::Error::Database(err) = why {
                    // we already posted this one :-)
                    if err.is_unique_violation() {
                        posted_cache.push(child.id.clone());
                    } else {
                        error!("DB Erro: {err}");
                    }
                } else {
                    error!("DB Error: {why}");
                }
                continue;
            }

            let preview_url =
                format!("https://share.redd.it/preview/post/{}", child.id);
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
                    image: Some(Image {
                        url: &preview_url,
                    }),
                    url: Some(child.url.unwrap()),
                    author: Author {
                        name: Cow::Borrowed(&author_url),
                        icon_url: Cow::Borrowed(REDDIT_LOGO),
                    },
                }],
            };
            if !first_start {
                let send =
                    client.post(&webhook_url).json(&message).send().await;
                if let Err(why) = send {
                    error!("Error sending: {why}");
                }
            }
        }
        if first_start {
            first_start = false;
        }
        std::thread::sleep(Duration::from_secs(60));
    }
}

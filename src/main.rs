use std::{borrow::Cow, net::SocketAddr, time::Duration};

use anyhow::anyhow;
use base64::{engine::general_purpose, Engine as _};
use reqwest::{header::HeaderMap, ClientBuilder};
use serde::Serialize;
use shuttle_runtime::Error;
use sqlx::SqlitePool;

use crate::model::*;

mod model;

const USER_AGENT: &str = "formula1discordredditapp:markus-dev@v0.2.0";

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

pub struct Runner {
    webhook_url: String,
    client_id: String,
    secret: String,
}

#[shuttle_runtime::async_trait]
impl shuttle_runtime::Service for Runner {
    async fn bind(
        mut self,
        _addr: SocketAddr,
    ) -> Result<(), Error> {
        let webhook_url = self.webhook_url;
        let client_id = self.client_id;
        let secret = self.secret;

        let encoded_creds =
            general_purpose::STANDARD.encode(format!("{client_id}:{secret}"));

        let mut headers = HeaderMap::new();
        headers.append(
            "Authorization",
            format!("BASIC {encoded_creds}").parse().expect("Header invalid"),
        );

        let client = ClientBuilder::new()
            .user_agent(USER_AGENT)
            .default_headers(headers)
            .build()
            .expect("clientbuilder");

        let database = SqlitePool::connect("sqlite://data.sqlite")
            .await
            .expect("database");

        let mut posted_cache: Vec<Cow<str>> = Vec::with_capacity(100);

        loop {
            let test = client.get("https://www.reddit.com/search.json?q=subreddit%3Aformula1%20flair%3Apost-news&source=recent&sort=hot&limit=100")
        .send().await;

            let request = match test {
                Ok(data) => data,
                Err(why) => {
                    println!("Error: {why}");
                    std::thread::sleep(Duration::from_secs(60));
                    continue;
                },
            };

            if let Err(why) = request.error_for_status_ref() {
                println!("Error: {why}");
                std::thread::sleep(Duration::from_secs(60));
                continue;
            }

            let data = request.json::<ReturnData>().await;
            let data = match data {
                Ok(data) => data,
                Err(why) => {
                    eprintln!("Error decoding data: {why}");
                    std::thread::sleep(Duration::from_secs(60));
                    continue;
                },
            };

            for child in data.data.children {
                let child = match child {
                    ApiListing::T3(t3) => t3,
                    _ => {
                        println!("unsupported T-Type found!");
                        continue;
                    },
                };

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
                    sqlx::query!("INSERT INTO posts (id) VALUES (?)", child.id)
                        .execute(&database)
                        .await;
                if let Err(why) = db_res {
                    if let sqlx::Error::Database(err) = why {
                        // we already posted this one :-)
                        if err.is_unique_violation() {
                            posted_cache.push(child.id.clone());
                        } else {
                            eprintln!("DB Erro: {err}");
                        }
                    } else {
                        eprintln!("DB Error: {why}");
                    }
                    continue;
                }

                let preview_url =
                    format!("https://share.redd.it/preview/post/{}", child.id);
                let author_url = format!("u/{} on r/formula1", child.author);
                let reddit_url =
                    format!("https://reddit.com{}", child.permalink);
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
                let send =
                    client.post(&webhook_url).json(&message).send().await;
                if let Err(why) = send {
                    eprintln!("Error sending: {why}");
                }
            }

            std::thread::sleep(Duration::from_secs(60));
        }
    }
}

macro_rules! secret {
    ($store: expr, $input:expr) => {
        match $store.get($input) {
            Some(secret) => secret,
            None => {
                return Err(anyhow!("Error getting secret {}", $input).into())
            },
        }
    };
}

#[shuttle_runtime::main]
async fn main(
    #[shuttle_secrets::Secrets] secrets_store: shuttle_secrets::SecretStore
) -> Result<impl shuttle_runtime::Service, shuttle_runtime::Error> {
    let runner = Runner {
        webhook_url: secret!(secrets_store, "WEBHOOK_URL"),
        client_id: secret!(secrets_store, "CLIENT_ID"),
        secret: secret!(secrets_store, "SECRET_KEY"),
    };
    Ok(runner)
}

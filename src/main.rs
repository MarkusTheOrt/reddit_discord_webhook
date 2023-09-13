use std::{borrow::Cow, time::Duration};

use reqwest::ClientBuilder;
use serde::Serialize;
use sqlx::SqlitePool;

use crate::model::*;

mod model;

const USER_AGENT: &str = "formula1discordredditapp:markus-dev@v0.1.0";

const REDDIT_LOGO: &str = "https://fia.ort.dev/reddit_logo.png";

const BANNED_URLS: [&'static str; 2] = [
    // no links to reddit.com allowed! (they like to do that a lot)
    "reddit.com",
    "redd.it",
];

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
async fn main() {

    let _ = dotenvy::dotenv();
    let webhook_url = std::env::var("WEBHOOK_URL").expect("webhook url in env");

    let client = ClientBuilder::new()
        .user_agent(USER_AGENT)
        .build()
        .expect("clientbuilder");

    let database = SqlitePool::connect("sqlite://data.sqlite")
        .await
        .expect("database");

    let mut posted_cache: Vec<Cow<str>> = Vec::with_capacity(100);
    loop {
        let test = client.get("https://www.reddit.com/search.json?q=subreddit%3Aformula1%20flair%3Apost-news&source=recent&sort=hot")
        .send().await;

        let request = match test {
            Ok(data) => data,
            Err(why) => {
                println!("Error: {why}");
                continue;
            }
        };

        if let Err(why) = request.error_for_status_ref() {
            println!("Error: {why}");
            return;
        }

        let data = request.json::<ReturnData>().await;
        let data = match data {
            Ok(data) => data,
            Err(why) => {
                eprintln!("Error decoding data: {why}");
                std::thread::sleep(Duration::from_secs(60));
                continue;
            }
        };

        for child in data.data.children {
            let child = match child {
                ApiListing::T3(t3) => t3,
                _ => {
                    println!("unsupported T-Type found!");
                    continue;
                }
            };
            let mut is_banned = false;
            let url = child.url.as_ref().unwrap();

            // skip over already posted shit.
            if posted_cache.contains(&child.id) || child.ups < 300 || child.url.is_none() {
                continue;
            }

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

            let db_res = sqlx::query!("INSERT INTO posts (id) VALUES (?)", child.id)
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

            let preview_url = format!("https://share.redd.it/preview/post/{}", child.id);
            let author_url = format!("u/{} on r/formula1", child.author);
            let mesasge = format!(
                "[go to Article on {}](<{}>)\n[go to Reddit post](<https://reddit.com{}>)",
                child.domain,
                child.url.as_ref().unwrap(),
                child.permalink
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
            let send = client.post(&webhook_url).json(&message).send().await;
            if let Err(why) = send {
                eprintln!("Error sending: {why}");
            }
        }

        std::thread::sleep(Duration::from_secs(60));
    }
}

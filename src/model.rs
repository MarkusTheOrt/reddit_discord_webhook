use std::borrow::Cow;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct ReturnData<'a> {
    kind: String,
    pub data: ReturnDataData<'a>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ReturnDataData<'a> {
    pub children: Vec<ApiListing<'a>>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum PostKind {
    Link,
    Image,
    Video,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum ApiListing<'a> {
    T1,         // Comment, NOT IMPLEMENTED
    T2,         // Account, NOT IMPLEMENTED
    T3(T3<'a>), // Link
    T4,         // Message, NOT IMPLEMENTED
    T5,         // Subreddit, NOT IMPLEMENTED
    T6,         // Award, NOT IMPLEMENTED
}

/// Struct representing a reddit Link API object.
#[derive(Serialize, Deserialize, Debug)]
pub struct T3<'a> {
    /// Subreddit name (without the r/).
    pub subreddit: String,
    /// I have no idea, there are no API docs regarding this.
    /// I can only imagine this is the text of the post if its not a link.
    pub selftext: Cow<'a, str>,
    /// while this suggests the authors full name, its just the t2 id....
    pub author_fullname: String,
    /// The title of the post
    pub title: Cow<'a, str>,
    /// Full subreddit name (including the r/)
    pub subreddit_name_prefixed: String,
    /// The number of downvotes this link has received.
    pub downs: i32,
    /// the number of upvotes this post received.
    pub ups: i32,
    /// the displayed score this post has (Upvotes - Downvotes?)
    pub score: i32,
    /// the url of the tiny thumbnail
    pub thumbnail: Cow<'a, str>,
    /// The (optional) url of the post. Although we only consider links to be valid here.
    pub url: Option<Cow<'a, str>>,
    /// The id of the post
    pub id: Cow<'a, str>,
    /// author name
    pub author: String,
    /// relative link to the post (need to add https://reddit.com/)
    pub permalink: Cow<'a, str>,
    /// the domain (www.example.com)
    pub domain: Cow<'a, str>,
    /// the time of post creation
    pub created_utc: f64,
}

impl<'a> T3<'_> {
    pub fn created(&self) -> u64 {
        return self.created_utc as u64;
    }
}

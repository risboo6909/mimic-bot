use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Source {
    pub(crate) messages: Vec<Message>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Message {
    pub(crate) id: i64,
    #[serde(rename = "type")]
    pub(crate) msg_type: String,
    pub(crate) date: String,
    pub(crate) from: Option<String>,
    #[serde(flatten)]
    pub(crate) text: Text,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub(crate) enum Text {
    Text { text: String },
    Link { text: Vec<TextOrLink> },
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Link {
    #[serde(rename = "type")]
    pub(crate) msg_type: String,
    pub(crate) text: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub(crate) enum TextOrLink {
    Link(Link),
    Text(String),
}

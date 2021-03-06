#![feature(str_split_once)]

mod brain;
mod config;

use futures::StreamExt;
use rand::Rng;
use reqwest::{redirect::Policy, Url};
use std::{thread, time, time::SystemTime};
use telegram_bot::*;

use brain::{Brain, UserName};
use config::Config;

lazy_static::lazy_static! {
    static ref CONFIG: Config = Config::new();
}

const REDIS_RETRY_DELAY: time::Duration = time::Duration::from_millis(5000);
const REDIS_RETRY_ATTEMPTS: usize = 5;

fn full_name(first_name: &str, last_name: Option<String>) -> String {
    match last_name {
        Some(last_name) => format!("{} {}", first_name, last_name),
        None => first_name.to_owned(),
    }
}

async fn handle_messages(api: Api, brain: &mut Brain, message: Message) -> Result<(), Error> {
    let chat_id = api.send(message.chat.get_chat()).await?.id();

    // try to read data for the given chat_id
    if let Err(err) = brain.read_from_redis(chat_id).await {
        api.send(message.text_reply(format!("Error loading chat data, reason: {}", err)))
            .await?;
    };

    if let MessageKind::Text { ref data, .. } = message.kind {
        let msg_text = data.as_str();

        if msg_text.starts_with("/learn") {
            let parts = msg_text.splitn(2, "/learn ").collect::<Vec<&str>>();

            if parts.len() < 2 {
                api.send(message.text_reply("Wrong syntax, use '/learn url_to_json user_name"))
                    .await?;
                // we don't care of this error anymore
                return Ok(());
            }

            let (uri, one_user) = match parts[1].trim().split_once(" ") {
                Some((uri, one_user)) => (uri, Some(UserName(one_user.to_owned()))),
                None => {
                    api.send(message.text_reply("User name must be provided"))
                        .await?;
                    // we don't care of this error anymore
                    return Ok(());
                }
            };

            let uri: Url = match uri.trim().parse() {
                Ok(uri) => uri,
                Err(_) => {
                    api.send(message.text_reply(format!("Error parsing uri: {}", uri)))
                        .await?;
                    return Ok(());
                }
            };

            api.send(message.text_reply("Downloading history data"))
                .await?;

            let client = reqwest::Client::builder()
                .redirect(Policy::limited(10))
                .user_agent("curl/7.64.1")
                .build()
                .expect("should be able to build reqwest client");

            let res = match client.get(uri).send().await {
                Ok(raw_data) => raw_data,
                Err(err) => {
                    api.send(message.text_reply(format!("Error downloading uri: {:?}", err)))
                        .await?;
                    return Ok(());
                }
            };

            let parsed = match res.json::<brain::types::Source>().await {
                Ok(parsed) => parsed,
                Err(err) => {
                    api.send(message.text_reply(format!("Error parsing josn: {:?}", err)))
                        .await?;
                    return Ok(());
                }
            };

            api.send(message.text_reply("Download completed")).await?;
            api.send(message.text_reply("Learning...")).await?;

            match brain.learn_from_hist(chat_id, parsed, one_user).await {
                Ok(proccessed) => {
                    api.send(message.text_reply(format!(
                        "Done learning, {} messages proccessed!",
                        proccessed
                    )))
                    .await?;
                }
                Err(err) => {
                    api.send(message.text_reply(format!("Error learning, reason: {}", err)))
                        .await?;
                }
            }
        } else if msg_text.starts_with("/say") {
            let parts = msg_text.split("/say ").collect::<Vec<&str>>();
            if parts.len() < 2 {
                api.send(message.text_reply("Wrong syntax, use '/say order (from 1 to 2)'"))
                    .await?;
                // we don't care of that error anymore
                return Ok(());
            }

            let order = parts[parts.len() - 1].parse::<usize>().unwrap_or(1);

            if let Some((name, resp)) = brain.gen_from_empty(chat_id, order) {
                api.send(message.text_reply(format!("{}: {} ", name, resp)))
                    .await?;
            }
        } else if !data.is_empty() {
            let full_name = UserName(full_name(
                &message.from.first_name,
                message.from.last_name.clone(),
            ));

            if brain.is_known_user(chat_id, &full_name) {
                brain.feed_message(chat_id, full_name, data, true).await;
            }

            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            if now - message.date as u64 > CONFIG.reply_timeout_sec {
                // don't reply to message older than REPLY_EXPIRE_TIME_SEC
                return Ok(());
            }

            let parts = data.split_whitespace().collect::<Vec<&str>>();
            let mut rng = rand::thread_rng();

            if let Some((name, resp)) = brain.gen_from_token(chat_id, parts[parts.len() - 1], 2) {
                // we've generated message base on the last word
                if rng.gen::<f64>() <= CONFIG.known_word_reply_prob {
                    api.send(message.text_reply(format!("{}: {} ", name, resp)))
                        .await?;
                }
            } else if let Some((name, resp)) = brain.gen_from_empty(chat_id, 2) {
                // just generate a random message
                if rng.gen::<f64>() <= CONFIG.reply_prob {
                    api.send(message.text_reply(format!("{}: {} ", name, resp)))
                        .await?;
                }
            }
        }
    }

    Ok(())
}

async fn try_get_connection(
    client: &redis::Client,
    mut try_no: usize,
) -> redis::RedisResult<redis::aio::Connection> {
    loop {
        let con = client.get_async_connection().await;

        if let Err(ref err) = con {
            try_no -= 1;
            if try_no == 0 {
                return con;
            }

            log::error!("error connecting to Redis: {}, retry in 5 seconds...", err);
            thread::sleep(REDIS_RETRY_DELAY);
        } else {
            return con;
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    pretty_env_logger::init();

    let api = Api::new(&CONFIG.telegram_bot_token);

    let redis_url = redis::parse_redis_url(&CONFIG.redis_addr).expect("unable to parse Redis url");
    let redis_conn_info = redis::ConnectionInfo {
        addr: Box::new(redis::ConnectionAddr::Tcp(
            redis_url.host().unwrap().to_string(),
            redis_url.port().unwrap(),
        )),
        db: 0,
        username: None,
        passwd: CONFIG.redis_passwd.clone(),
    };

    let client = redis::Client::open(redis_conn_info);
    if let Err(ref err) = client {
        panic!("error opening Redis connection: {}", err);
    }

    let con = try_get_connection(&client.unwrap(), REDIS_RETRY_ATTEMPTS).await;
    if let Err(err) = con {
        panic!("error connecting to Redis: {}", err);
    }

    let mut brain = Brain::new(1, 2).set_redis_con(con.unwrap());

    // Fetch new updates via long poll method
    let mut stream = api.stream();

    while let Some(update) = stream.next().await {
        // If the received update contains a new message...
        let update = update?;
        if let UpdateKind::Message(message) = update.kind {
            handle_messages(api.clone(), &mut brain, message).await?
        }
    }
    Ok(())
}

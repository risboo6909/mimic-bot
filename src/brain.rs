mod chains_pack;
pub(crate) mod types;

use chains_pack::Chains;
use rand::{self, seq::SliceRandom};
use redis::{aio::Connection, AsyncCommands};
use telegram_bot::ChatId;

use std::collections::{HashMap, HashSet};
use std::fmt::{self, Display, Formatter};

use super::CONFIG;

#[derive(Eq, Hash, Clone, Debug)]
pub(crate) struct UserName(pub(crate) String);

impl Display for UserName {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for UserName {
    fn from(s: &str) -> Self {
        UserName(s.to_owned())
    }
}

impl PartialEq for UserName {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_lowercase() == other.0.to_lowercase()
    }
}

pub(crate) struct Brain {
    min_order: usize,
    max_order: usize,
    msg_fed: usize,

    users: HashMap<ChatId, HashMap<UserName, Chains>>,
    loaded: HashSet<ChatId>,

    redis_con: Option<Connection>,
}

impl Brain {
    pub(crate) fn new(min_order: usize, max_order: usize) -> Self {
        Brain {
            min_order,
            max_order,
            msg_fed: 0,

            users: HashMap::new(),
            loaded: HashSet::new(),

            redis_con: None,
        }
    }

    pub(crate) fn set_redis_con(mut self, redis_con: Connection) -> Self {
        self.redis_con = Some(redis_con);
        self
    }

    /// Returns true if we already have some data for the given user
    /// and false otherwise
    pub(crate) fn is_known_user(&self, chat_id: ChatId, user_name: &UserName) -> bool {
        if let Some(chat_users) = &self.users.get(&chat_id) {
            if let Some(_) = chat_users.get(&user_name) {
                return true;
            }
        }
        false
    }

    fn redis_key(&self, chat_id: ChatId, user_name: UserName) -> String {
        format!("{}_{}", chat_id, user_name.0)
    }

    /// Writes new data per person to Redis
    async fn write_to_redis(&mut self, chat_id: ChatId, user_name: UserName) -> anyhow::Result<()> {
        if !self.is_known_user(chat_id, &user_name) {
            return Ok(());
        }

        let chains = &self
            .users
            .get(&chat_id)
            .expect("chat id data must exist on this step")
            .get(&user_name)
            .unwrap();

        let key = self.redis_key(chat_id, user_name);

        match self.redis_con {
            Some(ref mut redis_con) => {
                let raw = chains.serialize()?;
                redis_con.set(key, raw).await?;
            }
            None => {
                log::warn!("write_to_redis: can't save learn data, redis client is not set");
            }
        };

        Ok(())
    }

    /// Reads all data from redis for required chat
    pub(crate) async fn read_from_redis(&mut self, chat_id: ChatId) -> anyhow::Result<()> {
        // check whether we have data for this chat already loaded into memory
        if self.loaded.contains(&chat_id) {
            return Ok(());
        }

        log::info!("preparing to load data for chat id {}", chat_id);

        let mut user_data: HashMap<UserName, String> = HashMap::new();

        match self.redis_con {
            Some(ref mut redis_con) => {
                let key_patt = format!("{}*", i64::from(chat_id));
                let keys: Vec<String> = redis_con.keys(key_patt).await?;

                for key in keys {
                    let (_, name) = key.split_once('_').unwrap();
                    let raw = redis_con.get(&key).await?;
                    user_data.insert(UserName::from(name), raw);
                }
            }
            None => {
                log::warn!("read_from_redis: can't read data for chat, redis client is not ready");
            }
        };

        // deserialize
        for (name, raw) in user_data {
            log::info!("loading data for {}...", name);
            let chains = self.insert_new_chat_id_user(chat_id, &name);
            chains.deserialize(&raw);
        }

        self.loaded.insert(chat_id);
        log::info!("data for chat {} loaded", chat_id);

        Ok(())
    }

    fn insert_new_chat_id_user(&mut self, chat_id: ChatId, name: &UserName) -> &mut Chains {
        let min_order = self.min_order;
        let max_order = self.max_order;

        self.users
            .entry(chat_id)
            .or_insert_with(HashMap::new)
            .entry(name.clone())
            .or_insert_with(|| Chains::new(min_order, max_order))
    }

    pub(crate) async fn feed_message(
        &mut self,
        chat_id: ChatId,
        name: UserName,
        msg: &str,
        write_to_redis: bool,
    ) {

        let chains = self.insert_new_chat_id_user(chat_id, &name);
        chains.feed(msg);

        self.msg_fed += 1;

        if write_to_redis && (self.msg_fed % CONFIG.write_to_redis_freq) == 0 {
            if let Err(err) = self.write_to_redis(chat_id, name).await {
                log::error!("error writing new data to redis: {}", err);
            }
        }
    }

    pub(crate) async fn learn_from_hist(
        &mut self,
        chat_id: ChatId,
        input: types::Source,
        req_name: Option<UserName>,
    ) -> anyhow::Result<usize> {
        let mut proccessed = 0;
        let mut names = HashSet::new();

        for item in input.messages {
            if let Some(name) = item.from {

                if let Some(ref tmp) = req_name {
                    if *tmp != UserName(name.clone()) {
                        continue;
                    }
                }

                names.insert(name.clone());

                if let types::Text::Text { ref text } = item.text {
                    if !text.is_empty() {
                        self.feed_message(chat_id, UserName(name), text, false)
                            .await;
                        proccessed += 1;
                    }
                }
            }
        }

        // save to Redis
        match req_name {
            Some(name) => {
                self.write_to_redis(chat_id, name)
                    .await?
            }
            None => {
                for name in names {
                    self.write_to_redis(chat_id, UserName(name)).await?;
                }
            }
        }

        Ok(proccessed)
    }

    fn choose_user(&self, chat_id: ChatId) -> Option<UserName> {
        let users = self.users.get(&chat_id);
        let users_list = match users {
            Some(users) => users.keys().collect::<Vec<&UserName>>(),
            None => return None,
        };

        if users_list.is_empty() {
            return None;
        }

        let mut rng = rand::thread_rng();
        let choice = users_list.choose(&mut rng).unwrap();

        Some((*choice).clone())
    }

    /// Converts the output of `generate(...)` on a String chain to a single String.
    fn vec_to_string(&self, vec: &[String]) -> String {
        let mut ret = String::new();
        for s in vec {
            ret.push_str(&s);
            ret.push_str(" ");
        }
        let len = ret.len();
        if len > 0 {
            ret.truncate(len - 1);
        }
        ret
    }

    pub(crate) fn gen_from_token(
        &self,
        chat_id: ChatId,
        token: &str,
        order: usize,
    ) -> Option<(UserName, String)> {
        for _ in 0..CONFIG.max_gen_retries {
            let name = match self.choose_user(chat_id) {
                Some(name) => name,
                None => return None,
            };

            let chains = &self.users[&chat_id][&name];

            if let Some(tokens) = chains.gen_from_token(token).get(&order) {
                if tokens.len() < CONFIG.max_reply_tokens {
                    return Some((name.clone(), self.vec_to_string(tokens)));
                }
            }
        }

        None
    }

    pub(crate) fn gen_from_empty(
        &self,
        chat_id: ChatId,
        order: usize,
    ) -> Option<(UserName, String)> {
        for _ in 0..CONFIG.max_gen_retries {
            let name = match self.choose_user(chat_id) {
                Some(name) => name,
                None => return None,
            };

            let chains = &self.users[&chat_id][&name];

            if let Some(tokens) = chains.gen_from_empty().get(&order) {
                if tokens.len() < CONFIG.max_reply_tokens {
                    return Some((name.clone(), self.vec_to_string(tokens)));
                }
            }
        }

        None
    }
}

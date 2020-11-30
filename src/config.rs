use std::env;

const REDIS_ADDR: &str = "redis://127.0.0.1:5000/";

// don't reply to message older than REPLY_TIMEOUT_DEFAULT_SEC seconds
const REPLY_TIMEOUT_DEFAULT_SEC: &str = "5";
// probability to reply to any message
const REPLY_PROB_DEFAULT: &str = "0.01";
// probability to reply to message ending with known word
const KNOWN_WORD_REPLY_PROB: &str = "0.1";

// maximum number of attempts to generate uniqe and appropriate reply
const MAX_GEN_RETRIES: &str = "100";
// basically, the maximum number of words in a generated sentence
const MAX_REPLY_TOKENS: &str = "15";
// how often to dump database into Redis (every 10 new messages by default)
const WRITE_TO_REDIS_FREQ: &str = "10";

pub(crate) struct Config {
    pub(crate) redis_addr: String,
    pub(crate) redis_passwd: Option<String>,

    pub(crate) reply_timeout_sec: u64,
    pub(crate) reply_prob: f64,
    pub(crate) known_word_reply_prob: f64,

    pub(crate) max_gen_retries: usize,
    pub(crate) max_reply_tokens: usize,
    pub(crate) write_to_redis_freq: usize,

    pub(crate) telegram_bot_token: String,
}

impl Config {
    pub(crate) fn new() -> Self {
        Config {
            redis_addr: env::var("REDIS_ADDR").unwrap_or_else(|_| REDIS_ADDR.to_owned()),
            redis_passwd: env::var("REDIS_PASSWD").ok(),

            reply_timeout_sec: env::var("REPLY_TIMEOUT_SEC")
                .unwrap_or_else(|_| REPLY_TIMEOUT_DEFAULT_SEC.to_owned())
                .parse::<u64>()
                .expect("unable parse REPLY_TIMEOUT_SEC"),

            reply_prob: env::var("REPLY_PROB_DEFAULT")
                .unwrap_or_else(|_| REPLY_PROB_DEFAULT.to_owned())
                .parse::<f64>()
                .expect("unable parse REPLY_PROB_DEFAULT"),

            known_word_reply_prob: env::var("KNOWN_WORD_REPLY_PROB")
                .unwrap_or_else(|_| KNOWN_WORD_REPLY_PROB.to_owned())
                .parse::<f64>()
                .expect("unable parse KNOWN_WORD_REPLY_PROB"),

            max_gen_retries: env::var("MAX_GEN_RETRIES")
                .unwrap_or_else(|_| MAX_GEN_RETRIES.to_owned())
                .parse::<usize>()
                .expect("unable parse MAX_GEN_RETRIES"),

            max_reply_tokens: env::var("MAX_REPLY_TOKENS")
                .unwrap_or_else(|_| MAX_REPLY_TOKENS.to_owned())
                .parse::<usize>()
                .expect("unable parse MAX_REPLY_TOKENS"),

            write_to_redis_freq: env::var("write_to_redis_FREQ")
                .unwrap_or_else(|_| WRITE_TO_REDIS_FREQ.to_owned())
                .parse::<usize>()
                .expect("unable parse WRITE_TO_REDIS_FREQ"),

            telegram_bot_token: env::var("TELEGRAM_BOT_TOKEN").expect("TELEGRAM_BOT_TOKEN not set"),
        }
    }
}

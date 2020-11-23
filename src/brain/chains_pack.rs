use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use markov::Chain;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_yaml::Result;

const MAX_GEN_RETRIES: usize = 1000;

#[derive(Serialize, Deserialize)]
struct Inner {
    chains: HashMap<usize, Chain<String>>,

    // we only need to save hashes of string because we don't need to restore original messages
    // just check if message exists in known_messages
    known_messages: HashSet<u64>,
}

impl Inner {
    fn new(from_ord: usize, to_ord: usize) -> Self {
        let mut chains = HashMap::new();

        for order in from_ord..=to_ord {
            chains.insert(order, Chain::of_order(order));
        }

        Inner {
            chains,
            known_messages: HashSet::new(),
        }
    }

    fn get_hash(&self, tokens: &[String]) -> u64 {
        let mut hasher = DefaultHasher::default();
        tokens.hash(&mut hasher);
        hasher.finish()
    }

    fn remember_known(&mut self, tokens: &[String]) {
        let tmp = tokens
            .iter()
            .map(|t| t.to_lowercase())
            .collect::<Vec<String>>();
        for i in 0..tokens.len() {
            for j in i..tokens.len() + 1 {
                self.known_messages
                    .insert(self.get_hash(&Vec::from(&tmp[i..j])));
            }
        }
    }

    fn check_known(&self, generated: &[String]) -> bool {
        let tmp = generated
            .iter()
            .map(|t| t.to_lowercase())
            .collect::<Vec<String>>();
        self.known_messages.contains(&self.get_hash(&tmp))
    }
}

pub(crate) struct Chains {
    inner: Inner,
}

impl Chains {
    // Chains of orders starting from from_ord to to_ord inclusive
    pub(crate) fn new(from_ord: usize, to_ord: usize) -> Self {
        let inner = Inner::new(from_ord, to_ord);
        Chains { inner }
    }

    fn tokenize(&self, msg: &str) -> Vec<String> {
        msg.split_whitespace()
            .map(|s| {
                s.trim()
                    .to_lowercase()
                    .replace(&['\"', ';', ':', '\''][..], "")
            })
            .collect::<Vec<String>>()
    }

    pub(crate) fn feed(&mut self, msg: &str) -> Vec<String> {
        let tokens = self.tokenize(msg);
        for chain in &mut self.inner.chains.values_mut() {
            chain.feed(&tokens);
        }

        self.inner.remember_known(&tokens);
        tokens
    }

    fn gen_helper(&self, gen: impl Fn() -> Vec<String>) -> Option<Vec<String>> {
        // generate until we get something we don't know from learning set
        let mut rng = rand::thread_rng();
        for _ in 0..MAX_GEN_RETRIES {
            let generated = gen();
            if self.inner.check_known(&generated) {
                continue;
            }
            if rng.gen::<usize>() % 10 == 0 {
                return Some(generated);
            }
        }
        None
    }

    pub(crate) fn gen_from_token(&self, token: &str) -> HashMap<usize, Vec<String>> {
        let mut res = HashMap::new();
        for (order, chain) in &self.inner.chains {
            if chain.is_empty() {
                continue;
            };
            match self.gen_helper(|| chain.generate_from_token(token.to_owned())) {
                Some(generated) => res.insert(*order, generated),
                None => None,
            };
        }
        res
    }

    pub(crate) fn gen_from_empty(&self) -> HashMap<usize, Vec<String>> {
        let mut res = HashMap::new();
        for (order, chain) in &self.inner.chains {
            if chain.is_empty() {
                continue;
            };
            match self.gen_helper(|| chain.generate()) {
                Some(generated) => res.insert(*order, generated),
                None => None,
            };
        }
        res
    }

    pub(crate) fn serialize(&self) -> Result<String> {
        serde_yaml::to_string(&self.inner)
    }

    pub(crate) fn deserialize(&mut self, raw: &str) {
        self.inner = serde_yaml::from_str(raw).unwrap();
    }
}

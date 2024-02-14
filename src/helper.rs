use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::{Mutex, MutexGuard};

#[derive(Default)]
pub struct TwitchEventList {
    followers_list: Mutex<HashSet<String>>,
    subscribers_list: Mutex<HashSet<String>>,
}

impl TwitchEventList {
    pub async fn add_follower<T: Into<String>>(&self, follower: T) {
        let mut guard = self.followers_list.lock().await;

        guard.insert(follower.into());
    }

    pub async fn add_subscriber<T: Into<String>>(&self, subscriber: T) {
        let mut guard = self.subscribers_list.lock().await;

        guard.insert(subscriber.into());
    }

    pub fn get_followers(&self) -> MutexGuard<HashSet<String>> {
        self.followers_list.blocking_lock()
    }

    pub fn get_subscribers(&self) -> MutexGuard<HashSet<String>> {
        self.subscribers_list.blocking_lock()
    }
}

pub type ChattersList = Arc<Mutex<HashSet<String>>>;
pub type SafeTwitchEventList = Arc<TwitchEventList>;

pub fn create_new_chatters_list() -> ChattersList {
    Arc::new(Mutex::new(HashSet::new()))
}

pub fn create_new_twitch_event_list() -> SafeTwitchEventList {
    Arc::new(TwitchEventList::default())
}

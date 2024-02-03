use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;

pub(crate) type ChattersList = Arc<Mutex<HashSet<String>>>;

pub(crate) fn create_new_chatters_list() -> ChattersList {
    Arc::new(Mutex::new(HashSet::new()))
}

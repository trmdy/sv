use std::collections::HashMap;

pub struct RenderCache {
    pub list_rows: HashMap<(String, u16, bool), String>,
    pub detail: HashMap<(String, u16), Vec<String>>,
    pub markdown: HashMap<(String, u16), Vec<String>>,
    pub hits: u64,
    pub misses: u64,
}

impl RenderCache {
    pub fn new() -> Self {
        Self {
            list_rows: HashMap::new(),
            detail: HashMap::new(),
            markdown: HashMap::new(),
            hits: 0,
            misses: 0,
        }
    }

    pub fn invalidate_on_resize(&mut self) {
        self.list_rows.clear();
        self.detail.clear();
        self.markdown.clear();
    }

    pub fn invalidate_task(&mut self, task_id: &str) {
        self.list_rows.retain(|(id, _, _), _| id != task_id);
        self.detail.retain(|(id, _), _| id != task_id);
        self.markdown.retain(|(id, _), _| id != task_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalidate_on_resize_clears_entries() {
        let mut cache = RenderCache::new();
        cache.list_rows.insert(("sv-1".to_string(), 10, false), "row".to_string());
        cache.detail.insert(("sv-1".to_string(), 40), vec!["detail".to_string()]);
        cache.markdown.insert(("sv-1".to_string(), 40), vec!["md".to_string()]);
        cache.invalidate_on_resize();
        assert!(cache.list_rows.is_empty());
        assert!(cache.detail.is_empty());
        assert!(cache.markdown.is_empty());
    }
}

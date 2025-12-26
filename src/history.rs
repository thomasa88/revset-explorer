pub struct History {
    items: Vec<String>,
    max_size: usize,
    view_pos: usize,
    last_is_tentative: bool,
}

impl History {
    pub fn new(max_size: usize) -> Self {
        History {
            items: Vec::new(),
            max_size,
            view_pos: 0,
            last_is_tentative: false,
        }
    }

    pub fn add(&mut self, arg: &str, is_tentative: bool) {
        if self.items.last().map(|s| s.as_str()) == Some(arg) {
            return;
        }
        if self.items.len() >= self.max_size {
            self.items.remove(0);
        }
        if self.last_is_tentative {
            self.items.pop();
        }
        self.items.push(arg.to_owned());
        self.last_is_tentative = is_tentative;
        // View position is reset to the end after adding a new item
        self.view_pos = self.items.len() - 1;
    }

    pub fn get(&self) -> &str {
        &self.items[self.view_pos]
    }

    pub fn set_last_tentative(&mut self, is_tentative: bool) {
        self.last_is_tentative = is_tentative;
    }

    pub fn prev(&mut self) {
        self.view_pos = self.view_pos.saturating_sub(1);
    }

    pub fn next(&mut self) {
        if self.view_pos < self.items.len() - 1 {
            self.view_pos += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add() {
        let mut h = History::new(10);
        h.add("@", false);
        h.add("@-", false);
        h.add("@--", false);
        assert_eq!(h.items.len(), 3);
        assert_eq!(h.items, vec!["@", "@-", "@--"]);
    }

    #[test]
    fn duplicates_are_ignored() {
        let mut h = History::new(10);
        h.add("@-", false);
        h.add("@-", false);
        assert_eq!(h.items.len(), 1);
    }

    #[test]
    fn max_size() {
        let mut h = History::new(2);
        h.add("@", false);
        h.add("@-", false);
        h.add("@--", false);
        assert_eq!(h.items, vec!["@-", "@--"]);
    }

    #[test]
    fn get() {
        let mut h = History::new(10);
        h.add("@", false);
        h.add("@-", false);
        assert_eq!(h.get(), "@-");
    }

    #[test]
    fn prev() {
        let mut h = History::new(10);
        h.add("@", false);
        h.add("@-", false);
        h.add("@--", false);
        assert_eq!(h.get(), "@--");
        h.prev();
        assert_eq!(h.get(), "@-");
        h.prev();
        assert_eq!(h.get(), "@");
    }

    #[test]
    fn prev_saturate() {
        let mut h = History::new(10);
        h.add("@", false);
        h.prev();
        h.prev();
        assert_eq!(h.get(), "@");
    }

    #[test]
    fn next() {
        let mut h = History::new(10);
        h.add("@", false);
        h.add("@-", false);
        h.add("@--", false);
        h.prev();
        h.prev();
        assert_eq!(h.get(), "@");
        h.next();
        assert_eq!(h.get(), "@-");
        h.next();
        assert_eq!(h.get(), "@--");
    }

    #[test]
    fn next_saturate() {
        let mut h = History::new(10);
        h.add("@", false);
        h.next();
        assert_eq!(h.get(), "@");
    }

    #[test]
    fn add_tentative() {
        let mut h = History::new(10);
        h.add("@", false);
        h.add("@-", true);
        assert_eq!(h.items, vec!["@", "@-"]);
    }

    #[test]
    fn discard_tentative_on_new_add() {
        let mut h = History::new(10);
        h.add("@", true);
        h.add("@-", false);
        assert_eq!(h.items, vec!["@-"]);

        let mut h = History::new(10);
        h.add("@", true);
        h.add("@-", true);
        assert_eq!(h.items, vec!["@-"]);
    }

    #[test]
    fn discard_tentative_on_new_add_when_looking_at_the_past() {
        let mut h = History::new(10);
        h.add("::", false);
        h.add("@", true);
        h.prev();
        h.add("@--", false);
        assert_eq!(h.items, vec!["::", "@--"]);
    }

    #[test]
    fn keep_tentative_on_navigation() {
        let mut h = History::new(10);
        h.add("@", false);
        h.add("@-", true);
        h.prev();
        h.next();
        assert_eq!(h.items, vec!["@", "@-"]);

        let mut h = History::new(10);
        h.add("@", false);
        h.add("@-", true);
        h.next();
        assert_eq!(h.items, vec!["@", "@-"]);
    }
}

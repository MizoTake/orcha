#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoItem {
    pub id: usize,
    pub title: String,
    pub done: bool,
}

#[derive(Debug, Default)]
pub struct TodoList {
    items: Vec<TodoItem>,
    next_id: usize,
}

impl TodoList {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            next_id: 1,
        }
    }

    pub fn add(&mut self, title: impl Into<String>) -> usize {
        let id = self.next_id;
        self.next_id += 1;

        self.items.push(TodoItem {
            id,
            title: title.into(),
            done: false,
        });

        id
    }

    pub fn complete(&mut self, id: usize) -> bool {
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
            item.done = true;
            return true;
        }
        false
    }

    pub fn pending_titles(&self) -> Vec<&str> {
        self.items
            .iter()
            .filter(|item| !item.done)
            .map(|item| item.title.as_str())
            .collect()
    }

    pub fn stats(&self) -> (usize, usize) {
        let done = self.items.iter().filter(|item| item.done).count();
        (done, self.items.len())
    }
}

#[cfg(test)]
mod tests {
    use super::TodoList;

    #[test]
    fn add_item_assigns_incremental_id() {
        let mut list = TodoList::new();
        assert_eq!(list.add("design API"), 1);
        assert_eq!(list.add("write tests"), 2);
    }

    #[test]
    fn complete_marks_target_item_done() {
        let mut list = TodoList::new();
        let first = list.add("A");
        let second = list.add("B");

        assert!(list.complete(second));
        assert!(!list.complete(999));

        let pending = list.pending_titles();
        assert_eq!(pending, vec!["A"]);

        let (done, total) = list.stats();
        assert_eq!((done, total), (1, 2));
        assert!(first < second);
    }
}

use std::collections::BTreeMap;
use std::str::Chars;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock, Weak};

#[derive(Default, Clone)]
pub struct Node {
    pub value: String,
    pub children: BTreeMap<char, Arc<RwLock<Node>>>,
    pub parent: Weak<RwLock<Node>>,
    pub suffix_link: Weak<RwLock<Node>>,
    pub match_index: Option<usize>,
    pub is_leaf: bool,
}

#[derive(Default, Debug)]
pub struct Trie {
    pub(crate) root: Arc<RwLock<Node>>,
    pattern_size: AtomicUsize,
    usage: RwLock<BTreeMap<char, Vec<Arc<RwLock<Node>>>>>,
}

impl Trie {
    pub fn new() -> Self {
        Default::default()
    }
    pub fn add(&self, s: impl AsRef<str>) {
        let term = s.as_ref();
        let mut last_node = self.root.clone();
        let mut temp_last_node;

        for (i, c) in term.chars().enumerate() {
            temp_last_node = last_node;
            let parent = Arc::downgrade(&temp_last_node);
            let value = range_chars(0, i, term.chars());

            last_node = temp_last_node
                .write()
                .unwrap()
                .children
                .entry(c)
                .or_insert_with(|| {
                    Arc::new(RwLock::new(Node {
                        value: value.clone(),
                        children: BTreeMap::new(),
                        parent: parent.clone(),
                        suffix_link: Weak::new(),
                        match_index: None,
                        is_leaf: i == term.len() - 1,
                    }))
                })
                .clone();

            let suffix_link = get_suffix_link(c, last_node.clone());

            if !last_node.read().unwrap().value.is_empty() {
                let key = last_node.read().unwrap().value.chars().last().unwrap();
                self.usage
                    .write()
                    .unwrap()
                    .entry(key.clone())
                    .and_modify(|v| v.push(last_node.clone()))
                    .or_insert(vec![last_node.clone()]);
            }

            if let Some(nodes) = self
                .usage
                .write()
                .unwrap()
                .get_mut(&value.chars().last().unwrap())
            {
                for new_link in nodes {
                    let suffix_link = get_suffix_link(c, new_link.clone());
                    new_link.write().unwrap().suffix_link = Arc::downgrade(&suffix_link);
                }
            }
            last_node.write().unwrap().suffix_link = Arc::downgrade(&suffix_link);
        }
        last_node.write().unwrap().match_index = Some(self.pattern_size.load(Ordering::SeqCst));
        self.pattern_size.fetch_add(1, Ordering::SeqCst);
    }
}

static mut FIELDS: Vec<String> = vec![];

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        unsafe {
            if FIELDS.contains(&self.value) {
                return Ok(());
            }
            FIELDS.push(self.value.clone());
        }

        let parent = self.parent.upgrade();
        let parent_value = parent.map(|p| p.read().unwrap().value.clone());
        let suffix_link = self
            .suffix_link
            .upgrade()
            .map(|p| p.read().unwrap().value.clone());
        let suffix_link_value = suffix_link.map(|s| s.clone());

        f.debug_struct("Node")
            .field("value", &self.value)
            .field("children", &self.children)
            .field("parent", &parent_value)
            .field("suffix_link", &suffix_link_value)
            .field("match_index", &self.match_index)
            .field("is_leaf", &self.is_leaf)
            .finish()
    }
}

fn range_chars(start: usize, end: usize, chars: Chars) -> String {
    let mut s = String::with_capacity(end - start);
    for (i, ch) in chars.enumerate() {
        s.push(ch);
        if i == end {
            break;
        }
    }
    s
}

pub fn get_suffix_link(needle: char, node: Arc<RwLock<Node>>) -> Arc<RwLock<Node>> {
    let borrowed = node.read().unwrap();

    if borrowed.value.is_empty() {
        drop(borrowed);
        return node;
    }

    let parent = borrowed.parent.upgrade().unwrap();
    let parent_borrowed = parent.read().unwrap();
    if parent_borrowed.value.is_empty() {
        drop(parent_borrowed);
        return parent;
    }

    let suffix_link = parent_borrowed.suffix_link.upgrade().unwrap();
    if let Some(found) = suffix_link.read().unwrap().children.get(&needle) {
        return found.clone();
    }

    drop(parent_borrowed);
    get_suffix_link(needle, parent)
}

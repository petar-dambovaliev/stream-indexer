use crate::trie::Trie;
use arc_swap::ArcSwap;
use futures::{Future, Stream, StreamExt};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::select;
use tokio::sync::oneshot;
use tokio::sync::oneshot::{Receiver, Sender};

pub struct IndexBuilder<S> {
    read: Arc<ArcSwap<Trie>>,
    write: Arc<Trie>,
    buff: usize,
    stream: S,
}

impl<S> IndexBuilder<S>
where
    S: Stream,
{
    pub fn new(buff_size: usize, stream: S) -> Self {
        Self {
            read: Arc::new(Default::default()),
            write: Arc::new(Default::default()),
            buff: buff_size,
            stream,
        }
    }
    pub fn build(self) -> (Reader, Writer<S>) {
        (
            Reader {
                read: self.read.clone(),
            },
            Writer {
                read: self.read,
                write: self.write,
                buff: self.buff,
                stream: self.stream,
            },
        )
    }
}

#[allow(dead_code)]
pub struct Reader {
    read: Arc<ArcSwap<Trie>>,
}

impl Reader {
    pub fn find<T: AsRef<str>>(&self, s: T) -> Vec<Match> {
        let mut cur_node = self.read.load().root.clone();
        let mut matches = vec![];
        let mut i = 0;
        let chars: Vec<char> = s.as_ref().chars().collect();

        'outer: while i < chars.len() {
            let switcharoo_clone = cur_node.clone();
            let cur_ref_node = switcharoo_clone.read().unwrap().clone();
            let next = cur_ref_node.children.get(&chars[i]);

            cur_node = match next {
                Some(n) => n.clone(),
                None => match cur_ref_node.suffix_link.upgrade() {
                    Some(upg) => {
                        cur_node = upg;
                        continue 'outer;
                    }
                    None => {
                        cur_node = self.read.load().root.clone();
                        i += 1;
                        continue 'outer;
                    }
                },
            };

            if let Some(m) = cur_node.read().unwrap().match_index {
                matches.push(Match {
                    index: m,
                    size: cur_node.read().unwrap().value.chars().count(),
                    end: i,
                });
            }

            i += 1;
        }

        matches
    }
}

#[derive(Default, Debug, Clone, Eq, PartialEq)]
pub struct Match {
    pub index: usize,
    pub size: usize,
    pub end: usize,
}

impl Match {
    pub fn start(&self) -> usize {
        self.end - (self.size - 1)
    }
}

#[allow(dead_code)]
pub struct Writer<S> {
    read: Arc<ArcSwap<Trie>>,
    write: Arc<Trie>,
    buff: usize,
    stream: S,
}

impl<S> Writer<S>
where
    S: Stream<Item = String> + Unpin,
{
    pub fn build(mut self) -> (impl Future<Output = ()>, Sender<()>) {
        let (send, mut recv): (Sender<()>, Receiver<()>) = oneshot::channel();
        let fut = async move {
            let mut buffer = VecDeque::with_capacity(self.buff);
            loop {
                select! {
                    next = self.stream.next() => {
                        let next = match next {
                            Some(n) => n,
                            None => return
                        };
                        buffer.push_back(next.clone());
                        self.write.add(next);

                         if self.buff == buffer.len() {
                            let new_write = self.read.swap(self.write.clone());

                            while let Some(s) = buffer.pop_front() {
                                new_write.add(s);
                            }
                            println!("store read");
                            self.write = new_write;
                        }
                    }
                    _ = &mut recv => {
                        println!("operation completed");
                        return;
                    }
                }
            }
        };

        (fut, send)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;

    fn make_stream<T: AsRef<str>>(v: Vec<T>) -> impl Stream<Item = String> + Unpin {
        Box::pin(stream::iter(v).then(|i| async move { i.as_ref().to_string() }))
    }

    #[tokio::test]
    async fn crash_index() {
        let trie_terms = make_stream(vec!["TNF-α", "α-Blocker", "asd", "qwe", "qsdqsdqd"]);
        let (reader, writer) = IndexBuilder::new(2, trie_terms).build();

        let (write_future, cancel) = writer.build();
        tokio::spawn(write_future);
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        let matches = reader.find("sdf asd");
        assert_eq!(1, matches.len())
    }

    // #[test]
    // fn clear_html_prefix_replace() {
    //     let a = r#"und Streptokokken<li>trailing"#;
    //     let trie_terms = vec!["Streptokokken"];
    //     let finder = LinkerBuilder::new().build(trie_terms);
    //     let replaced = finder.link(a);
    //
    //     assert_eq!(
    //         "und <a data-autolink=true>Streptokokken</a><li>trailing",
    //         replaced
    //     );
    // }
    //
    // #[test]
    // fn crash() {
    //     let a = r#"Trinkwasser- Salmonella enterica Serovar Typhi"#;
    //     let trie_terms = vec![
    //         "Salmonella enterica",
    //         "Salmonella enterica Serovar Typhi",
    //         "Trinkwasser",
    //         "Salmonellen",
    //         "Gramnegative",
    //         "Paratyphus",
    //         "Salmonella enterica",
    //         "Typhus",
    //     ];
    //     let finder = LinkerBuilder::new().build(trie_terms);
    //     let replaced = finder.link(a);
    //
    //     assert_eq!(
    //         r#"<a data-autolink=true>Trinkwasser</a> <a data-autolink=true>Salmonella enterica Serovar Typhi</a>"#,
    //         replaced
    //     );
    // }
    //
    // #[test]
    // fn finds_single_simple_match() {
    //     let trie_terms = vec!["a", "ba", "bca", "c", "caa"];
    //     let finder = LinkerBuilder::new().build(trie_terms);
    //     let r_str: Vec<_> = "abcd bca abcd".chars().collect();
    //     let matches = finder.find(&r_str);
    //
    //     assert_eq!(
    //         matches,
    //         vec![Match {
    //             value: "bca".to_string(),
    //             end: 7,
    //             inner_html: vec![],
    //             size: 3
    //         }]
    //     )
    // }
    //
    // #[test]
    // fn unmatched() {
    //     let a = r#"die Kehlkopfdiphtherie"#;
    //
    //     let trie_terms = vec!["die Kehlkopfdipht", "Kehlkopfdiphtherie"];
    //     let finder = LinkerBuilder::new().build(trie_terms);
    //     let replaced = finder.link(a);
    //
    //     assert_eq!(
    //         r#"die <a data-autolink=true>Kehlkopfdiphtherie</a>"#,
    //         replaced
    //     );
    // }
    //
    // #[test]
    // fn clear_html_prefix() {
    //     let a: Vec<_> = r#"<li>und Streptokokken</li></ul>"#.chars().collect();
    //     let trie_terms = vec!["Streptokokken"];
    //     let finder = LinkerBuilder::new().build(trie_terms);
    //     let matches = finder.find(&a);
    //
    //     assert_eq!(
    //         matches,
    //         vec![Match {
    //             value: "Streptokokken".to_string(),
    //             end: 20,
    //             inner_html: vec![],
    //             size: 13
    //         }]
    //     );
    // }
    //
    // #[test]
    // fn finds_longest_possible_match() {
    //     let trie_terms = vec!["abc", "def", "abc def"];
    //     let finder = LinkerBuilder::new().build(trie_terms);
    //     let r_str: Vec<_> = "asd abc def".chars().collect();
    //     let matches = finder.find(&r_str);
    //
    //     assert_eq!(
    //         matches,
    //         vec![Match {
    //             value: "abc def".to_string(),
    //             end: 10,
    //             inner_html: vec![],
    //             size: 7
    //         }]
    //     )
    // }
    //
    // #[test]
    // fn matches_single_term() {
    //     let links = vec!["a", "ba", "bca", "c", "caa"];
    //     let finder = LinkerBuilder::new().build(links);
    //
    //     let r_str: Vec<_> = "bca".chars().collect();
    //     let matches = finder.find(&r_str);
    //
    //     assert_eq!(
    //         matches,
    //         vec![Match {
    //             value: "bca".to_string(),
    //             size: 3,
    //             end: 2,
    //             inner_html: vec![]
    //         }]
    //     )
    // }
    //
    // #[test]
    // fn matches_only_once_per_char() {
    //     let trie_terms = vec!["a", "aa", "aaa", "aaaa", "aaaaa"];
    //     let finder = LinkerBuilder::new().build(trie_terms);
    //     let r_str: Vec<_> = "aaaa".chars().collect();
    //     let matches = finder.find(&r_str);
    //
    //     assert_eq!(
    //         matches,
    //         vec![Match {
    //             value: "aaaa".to_string(),
    //             size: 4,
    //             end: 3,
    //             inner_html: vec![]
    //         }]
    //     )
    // }
    //
    // #[test]
    // fn finds_multiple_simple_matches() {
    //     let trie_terms = vec!["a", "ba", "bca", "c", "caa"];
    //     let finder = LinkerBuilder::new().build(trie_terms);
    //     let r_str: Vec<_> = "a abcd bca abcd".chars().collect();
    //     let matches = finder.find(&r_str);
    //
    //     assert_eq!(
    //         matches,
    //         vec![
    //             Match {
    //                 value: "a".to_string(),
    //                 size: 1,
    //                 end: 0,
    //                 inner_html: vec![]
    //             },
    //             Match {
    //                 value: "bca".to_string(),
    //                 size: 3,
    //                 end: 9,
    //                 inner_html: vec![]
    //             }
    //         ]
    //     )
    // }
    //
    // #[test]
    // fn finds_correct_matches_on_branch_failure() {
    //     let trie_terms = vec!["abc defgh", "abc", "defij"];
    //     let finder = LinkerBuilder::new().build(trie_terms);
    //     let r_str: Vec<_> = "abc defij".chars().collect();
    //     let matches = finder.find(&r_str);
    //
    //     assert_eq!(
    //         matches,
    //         vec![
    //             Match {
    //                 value: "abc".to_string(),
    //                 size: 3,
    //                 end: 2,
    //                 inner_html: vec![]
    //             },
    //             Match {
    //                 value: "defij".to_string(),
    //                 size: 5,
    //                 end: 8,
    //                 inner_html: vec![]
    //             }
    //         ]
    //     )
    // }
    //
    // #[test]
    // fn finds_match_with_inner_html() {
    //     let trie_terms = vec!["a", "ba", "bca", "c", "caa"];
    //     let finder = LinkerBuilder::new().build(trie_terms);
    //     let r_str: Vec<_> = "bcc <span>bca</span>".chars().collect();
    //     let matches = finder.find(&r_str);
    //
    //     assert_eq!(
    //         matches,
    //         vec![Match {
    //             value: "bca".to_string(),
    //             size: 9,
    //             end: 12,
    //             inner_html: vec![(4, 10)]
    //         }]
    //     )
    // }
}

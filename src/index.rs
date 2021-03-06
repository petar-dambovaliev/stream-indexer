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
    async fn full_match() {
        let trie_terms = make_stream(vec!["peter", "john", "eric", "johnson"]);
        let (reader, writer) = IndexBuilder::new(4, trie_terms).build();

        let (write_future, cancel) = writer.build();
        tokio::spawn(write_future);

        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let matches = reader.find("peter johnson");
            if !matches.is_empty() {
                assert_eq!(3, matches.len());
                assert_eq!(0, matches[0].index);
                assert_eq!(1, matches[1].index);
                assert_eq!(3, matches[2].index);
                return;
            }
        }
    }
}

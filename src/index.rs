use crate::trie::Trie;
use arc_swap::ArcSwap;
use futures::{Future, Stream, StreamExt};
use std::sync::Arc;
use tokio::select;
use tokio::sync::oneshot;
use tokio::sync::oneshot::{Receiver, Sender};

pub struct IndexBuilder<S> {
    read: Arc<ArcSwap<Trie>>,
    write: Arc<ArcSwap<Trie>>,
    buff: Vec<String>,
    stream: S,
}

impl<S> IndexBuilder<S>
where
    S: Stream,
{
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

#[allow(dead_code)]
pub struct Writer<S> {
    read: Arc<ArcSwap<Trie>>,
    write: Arc<ArcSwap<Trie>>,
    buff: Vec<String>,
    stream: S,
}

impl<S> Writer<S>
where
    S: Stream<Item = String> + Unpin,
{
    #[allow(dead_code)]
    fn build(mut self) -> (impl Future<Output = ()>, Sender<()>) {
        let (send, mut recv): (Sender<()>, Receiver<()>) = oneshot::channel();
        let fut = async move {
            loop {
                select! {
                    next = self.stream.next() => {
                        self.write.load().add(next.unwrap());

                         if self.buff.capacity() == self.buff.len() {
                            let write = self.write.swap(self.read.load_full());
                            self.read.store(write);
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

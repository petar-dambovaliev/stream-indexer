use crate::trie::Trie;
use std::cmp::Ordering;
use std::collections::VecDeque;
use std::fmt::Write;
use std::iter::FromIterator;
use std::sync::Arc;

pub struct LinkerBuilder;

impl LinkerBuilder {
    pub fn new() -> Self {
        Self
    }

    pub fn build<P>(&self, input_terms: Vec<P>) -> Linker<P>
    where
        P: AsRef<str>,
    {
        let trie = Arc::new(Trie::new());

        for input_term in &input_terms {
            trie.add(input_term);
        }

        Linker { trie, input_terms }
    }
}

#[derive(Default, Debug, Clone, Eq, PartialEq)]
pub struct Match {
    pub value: String,
    pub inner_html: Vec<(usize, usize)>,
    pub size: usize,
    pub end: usize,
}

impl Match {
    pub fn start(&self) -> usize {
        self.end - (self.size - 1)
    }
}

pub struct Linker<P>
where
    P: AsRef<str>,
{
    trie: Arc<Trie>,
    input_terms: Vec<P>,
}

impl<P> Linker<P>
where
    P: AsRef<str>,
{
    pub fn link(&self, r: &str) -> String {
        let chars: Vec<_> = r.chars().collect();
        let matches = self.find(&chars);
        //println!("{:#?}", matches);

        if matches.is_empty() {
            return r.to_string();
        }

        let mut replaced = String::with_capacity(r.len() + matches.len() * 4);

        let mut start = 0;

        for m in matches {
            // if start > m.start() {
            //     println!("string: {}", r);
            //     println!("matches: {:#?}", mm);
            // }
            replaced
                .write_str(&String::from_iter(&chars[start..m.start()]))
                .unwrap();
            replaced
                .write_str(&format!("<a data-autolink=true>{}</a>", m.value))
                .unwrap();
            start = m.start() + m.size + 1
        }

        //println!("replaced: {}", replaced.to_string());
        if start - 1 < r.len() {
            replaced
                .write_str(&String::from_iter(&chars[start - 1..]))
                .unwrap();
        }

        //println!("replaced: {}", replaced.to_string());

        replaced
    }

    fn remove_partial_matches(&self, chars: &Vec<char>, matches: &mut Vec<Match>) {
        //println!("{:#?}", chars);
        matches.retain(|m| {
            let start = m.start();
            if start > 0 && chars[start - 1].is_alphanumeric() {
                //println!("start: removing {:#?}", m);
                return false;
            }

            if m.end < chars.len() - 1 && chars[m.end + 1].is_alphanumeric() {
                //println!("end: removing {:#?}", m);
                return false;
            }

            true
        });
    }
    fn dedup_matches(&self, mut matches: Vec<Match>) -> Vec<Match> {
        matches.sort_by(|a, b| {
            if a.end - (a.size - 1) < b.end - (b.size - 1) {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        });

        if matches.is_empty() {
            return vec![];
        }

        matches.sort_by(|a, b| {
            if a.start() <= b.start() && a.end > b.end {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        });

        let mut queue = VecDeque::from(matches);

        let mut dedup_matches = vec![queue.pop_front().unwrap()];
        let mut i = 0;

        for m in queue {
            let match_a = &dedup_matches[i];

            // here we have the last character of a match being the first character of the next
            if m.end == match_a.start() || match_a.end == m.start() {
                continue;
            }
            // if matches are overlapping
            if m.start() == match_a.start() && m.end < match_a.end {
                continue;
            }
            // if matches are overlapping
            if (m.start() >= match_a.start() && m.start() < match_a.end)
                || (m.end > match_a.start() && m.start() <= match_a.start())
            {
                continue;
            }
            dedup_matches.push(m);
            i += 1;
        }

        dedup_matches.sort_by(|a, b| {
            if a.start() < b.start() {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        });
        dedup_matches
    }

    pub fn find(&self, chars: &Vec<char>) -> Vec<Match> {
        let mut cur_node = self.trie.root.clone();
        let mut matches = vec![];
        let mut inner_html = vec![];
        let mut i = 0;

        'outer: while i < chars.len() {
            if chars[i] == '<' {
                let start = i;
                while i < chars.len() {
                    if chars[i] == '>' {
                        i += 1;
                        let end = i;
                        inner_html.push((start, end));
                        continue 'outer;
                    }
                    i += 1;
                }
            }

            let switcharoo_clone = cur_node.clone();
            let cur_ref_node = switcharoo_clone.read().unwrap().clone();
            let next = cur_ref_node.children.get(&chars[i]);

            cur_node = match next {
                Some(n) => n.clone(),
                None => {
                    inner_html.clear();

                    match cur_ref_node.suffix_link.upgrade() {
                        Some(upg) => {
                            cur_node = upg;
                            continue 'outer;
                        }
                        None => {
                            cur_node = self.trie.root.clone();
                            i += 1;
                            continue 'outer;
                        }
                    }
                }
            };

            //println!("take suffix link: {:#?}", cur_node.borrow().value);
            if let Some(m) = cur_node.read().unwrap().match_index {
                let mut html_size = 0;
                for (s, e) in &inner_html {
                    html_size += e - s;
                }

                matches.push(Match {
                    value: self.input_terms[m].as_ref().to_string(),
                    inner_html: std::mem::take(&mut inner_html),
                    size: html_size + cur_node.read().unwrap().value.chars().count(),
                    end: i,
                });
            }

            i += 1;
        }

        self.remove_partial_matches(chars, &mut matches);
        //println!("{:#?}", matches);
        let deduplicated = self.dedup_matches(matches);
        deduplicated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_index() {
        let a = r#"TNF-α-Blocker"#;

        let trie_terms = vec!["TNF-α", "α-Blocker"];
        let finder = LinkerBuilder::new().build(trie_terms);
        let chars: Vec<_> = a.chars().collect();
        let m = finder.find(&chars);

        assert_eq!(1, m.len())
    }

    #[test]
    fn clear_html_prefix_replace() {
        let a = r#"und Streptokokken<li>trailing"#;
        let trie_terms = vec!["Streptokokken"];
        let finder = LinkerBuilder::new().build(trie_terms);
        let replaced = finder.link(a);

        assert_eq!(
            "und <a data-autolink=true>Streptokokken</a><li>trailing",
            replaced
        );
    }

    #[test]
    fn crash() {
        let a = r#"Trinkwasser- Salmonella enterica Serovar Typhi"#;
        let trie_terms = vec![
            "Salmonella enterica",
            "Salmonella enterica Serovar Typhi",
            "Trinkwasser",
            "Salmonellen",
            "Gramnegative",
            "Paratyphus",
            "Salmonella enterica",
            "Typhus",
        ];
        let finder = LinkerBuilder::new().build(trie_terms);
        let replaced = finder.link(a);

        assert_eq!(
            r#"<a data-autolink=true>Trinkwasser</a> <a data-autolink=true>Salmonella enterica Serovar Typhi</a>"#,
            replaced
        );
    }

    #[test]
    fn finds_single_simple_match() {
        let trie_terms = vec!["a", "ba", "bca", "c", "caa"];
        let finder = LinkerBuilder::new().build(trie_terms);
        let r_str: Vec<_> = "abcd bca abcd".chars().collect();
        let matches = finder.find(&r_str);

        assert_eq!(
            matches,
            vec![Match {
                value: "bca".to_string(),
                end: 7,
                inner_html: vec![],
                size: 3
            }]
        )
    }

    #[test]
    fn unmatched() {
        let a = r#"die Kehlkopfdiphtherie"#;

        let trie_terms = vec!["die Kehlkopfdipht", "Kehlkopfdiphtherie"];
        let finder = LinkerBuilder::new().build(trie_terms);
        let replaced = finder.link(a);

        assert_eq!(
            r#"die <a data-autolink=true>Kehlkopfdiphtherie</a>"#,
            replaced
        );
    }

    #[test]
    fn clear_html_prefix() {
        let a: Vec<_> = r#"<li>und Streptokokken</li></ul>"#.chars().collect();
        let trie_terms = vec!["Streptokokken"];
        let finder = LinkerBuilder::new().build(trie_terms);
        let matches = finder.find(&a);

        assert_eq!(
            matches,
            vec![Match {
                value: "Streptokokken".to_string(),
                end: 20,
                inner_html: vec![],
                size: 13
            }]
        );
    }

    #[test]
    fn finds_longest_possible_match() {
        let trie_terms = vec!["abc", "def", "abc def"];
        let finder = LinkerBuilder::new().build(trie_terms);
        let r_str: Vec<_> = "asd abc def".chars().collect();
        let matches = finder.find(&r_str);

        assert_eq!(
            matches,
            vec![Match {
                value: "abc def".to_string(),
                end: 10,
                inner_html: vec![],
                size: 7
            }]
        )
    }

    #[test]
    fn matches_single_term() {
        let links = vec!["a", "ba", "bca", "c", "caa"];
        let finder = LinkerBuilder::new().build(links);

        let r_str: Vec<_> = "bca".chars().collect();
        let matches = finder.find(&r_str);

        assert_eq!(
            matches,
            vec![Match {
                value: "bca".to_string(),
                size: 3,
                end: 2,
                inner_html: vec![]
            }]
        )
    }

    #[test]
    fn matches_only_once_per_char() {
        let trie_terms = vec!["a", "aa", "aaa", "aaaa", "aaaaa"];
        let finder = LinkerBuilder::new().build(trie_terms);
        let r_str: Vec<_> = "aaaa".chars().collect();
        let matches = finder.find(&r_str);

        assert_eq!(
            matches,
            vec![Match {
                value: "aaaa".to_string(),
                size: 4,
                end: 3,
                inner_html: vec![]
            }]
        )
    }

    #[test]
    fn finds_multiple_simple_matches() {
        let trie_terms = vec!["a", "ba", "bca", "c", "caa"];
        let finder = LinkerBuilder::new().build(trie_terms);
        let r_str: Vec<_> = "a abcd bca abcd".chars().collect();
        let matches = finder.find(&r_str);

        assert_eq!(
            matches,
            vec![
                Match {
                    value: "a".to_string(),
                    size: 1,
                    end: 0,
                    inner_html: vec![]
                },
                Match {
                    value: "bca".to_string(),
                    size: 3,
                    end: 9,
                    inner_html: vec![]
                }
            ]
        )
    }

    #[test]
    fn finds_correct_matches_on_branch_failure() {
        let trie_terms = vec!["abc defgh", "abc", "defij"];
        let finder = LinkerBuilder::new().build(trie_terms);
        let r_str: Vec<_> = "abc defij".chars().collect();
        let matches = finder.find(&r_str);

        assert_eq!(
            matches,
            vec![
                Match {
                    value: "abc".to_string(),
                    size: 3,
                    end: 2,
                    inner_html: vec![]
                },
                Match {
                    value: "defij".to_string(),
                    size: 5,
                    end: 8,
                    inner_html: vec![]
                }
            ]
        )
    }

    #[test]
    fn finds_match_with_inner_html() {
        let trie_terms = vec!["a", "ba", "bca", "c", "caa"];
        let finder = LinkerBuilder::new().build(trie_terms);
        let r_str: Vec<_> = "bcc <span>bca</span>".chars().collect();
        let matches = finder.find(&r_str);

        assert_eq!(
            matches,
            vec![Match {
                value: "bca".to_string(),
                size: 9,
                end: 12,
                inner_html: vec![(4, 10)]
            }]
        )
    }
}

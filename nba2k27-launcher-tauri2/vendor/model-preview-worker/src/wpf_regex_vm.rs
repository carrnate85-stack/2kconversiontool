//! Lazy, bounded backtracking: accept the first successful path without
//! enumerating every alternative or turning easy Unicode matches exponential.
use super::*;
enum Op {
    Accept,
    Lit(u16, bool),
    Any(bool),
    Class(Set),
    Anchor(Anchor),
    Jump(usize),
    Split(usize, usize),
    Open(usize),
    Close(usize),
    Balance(Option<usize>, usize),
    Backref(usize, bool),
    Look(Box<Program>, bool),
    AtomicStart,
    AtomicEnd,
    Condition(usize, usize, usize),
    AssertCondition(Box<Program>, usize, usize),
    Reset(usize),
    Check {
        id: usize,
        lo: usize,
        hi: usize,
        greedy: bool,
        body: usize,
        after: usize,
    },
    RepeatEnd {
        id: usize,
        lo: usize,
        check: usize,
        after: usize,
    },
}
pub(super) struct Program {
    ops: Vec<Op>,
    reverse: bool,
    counters: usize,
}
struct Builder {
    ops: Vec<Op>,
    reverse: bool,
}
impl Builder {
    fn node(&mut self, node: &Node, next: &mut usize) -> Result<()> {
        match node {
            Node::Empty | Node::Directive => {}
            Node::Lit(c, i) => self.ops.push(Op::Lit(*c, *i)),
            Node::Any(s) => self.ops.push(Op::Any(*s)),
            Node::Class(s) => self.ops.push(Op::Class(s.clone())),
            Node::Anchor(a) => self.ops.push(Op::Anchor(*a)),
            Node::Seq(children) => {
                for n in 0..children.len() {
                    self.node(
                        &children[if self.reverse {
                            children.len() - 1 - n
                        } else {
                            n
                        }],
                        next,
                    )?;
                }
            }
            Node::Alt(children) => {
                let mut jumps = vec![];
                for (n, child) in children.iter().enumerate() {
                    let split = if n + 1 < children.len() {
                        let p = self.ops.len();
                        self.ops.push(Op::Split(p + 1, 0));
                        Some(p)
                    } else {
                        None
                    };
                    self.node(child, next)?;
                    if let Some(p) = split {
                        jumps.push(self.ops.len());
                        self.ops.push(Op::Jump(0));
                        let after = self.ops.len();
                        self.ops[p] = Op::Split(p + 1, after);
                    }
                }
                let end = self.ops.len();
                for p in jumps {
                    self.ops[p] = Op::Jump(end);
                }
            }
            Node::Capture(slot, child) => {
                self.ops.push(Op::Open(*slot));
                self.node(child, next)?;
                self.ops.push(Op::Close(*slot));
            }
            Node::Balance(destination, source, child) => {
                self.ops.push(Op::Open(*source));
                self.node(child, next)?;
                self.ops.push(Op::Balance(*destination, *source));
            }
            Node::Atomic(child) => {
                self.ops.push(Op::AtomicStart);
                self.node(child, next)?;
                self.ops.push(Op::AtomicEnd);
            }
            Node::Look(child, positive, reverse) => self.ops.push(Op::Look(
                Box::new(program(child, *reverse, next)?),
                *positive,
            )),
            Node::Backref(slot, i) => self.ops.push(Op::Backref(*slot, *i)),
            Node::Repeat(child, lo, hi, greedy) => {
                let id = *next;
                *next += 1;
                self.ops.push(Op::Reset(id));
                let check = self.ops.len();
                self.ops.push(Op::Accept);
                let body = self.ops.len();
                self.node(child, next)?;
                let end = self.ops.len();
                self.ops.push(Op::Accept);
                let after = self.ops.len();
                self.ops[check] = Op::Check {
                    id,
                    lo: *lo,
                    hi: *hi,
                    greedy: *greedy,
                    body,
                    after,
                };
                self.ops[end] = Op::RepeatEnd {
                    id,
                    lo: *lo,
                    check,
                    after,
                };
            }
            Node::Condition(slot, yes, no) => {
                let p = self.ops.len();
                self.ops.push(Op::Accept);
                let yes_pc = self.ops.len();
                self.node(yes, next)?;
                let jump = self.ops.len();
                self.ops.push(Op::Jump(0));
                let no_pc = self.ops.len();
                self.node(no, next)?;
                let end = self.ops.len();
                self.ops[p] = Op::Condition(*slot, yes_pc, no_pc);
                self.ops[jump] = Op::Jump(end);
            }
            Node::AssertCondition(condition, yes, no) => {
                let condition = program(condition, self.reverse, next)?;
                let p = self.ops.len();
                self.ops.push(Op::Accept);
                let yes_pc = self.ops.len();
                self.node(yes, next)?;
                let jump = self.ops.len();
                self.ops.push(Op::Jump(0));
                let no_pc = self.ops.len();
                self.node(no, next)?;
                let end = self.ops.len();
                self.ops[p] = Op::AssertCondition(Box::new(condition), yes_pc, no_pc);
                self.ops[jump] = Op::Jump(end);
            }
            Node::RawRef(..) | Node::RawCondition(..) | Node::RawBalance(..) => {
                bail!("Unresolved regex instruction")
            }
        }
        Ok(())
    }
}
fn program(node: &Node, reverse: bool, next: &mut usize) -> Result<Program> {
    let mut b = Builder {
        ops: vec![],
        reverse,
    };
    b.node(node, next)?;
    b.ops.push(Op::Accept);
    Ok(Program {
        ops: b.ops,
        reverse,
        counters: 0,
    })
}
pub(super) fn compile(node: &Node) -> Result<Program> {
    let mut count = 0;
    let mut p = program(node, false, &mut count)?;
    p.counters = count;
    Ok(p)
}
#[derive(Clone, Copy, Default)]
struct Counter {
    count: usize,
    previous: usize,
}
#[derive(Clone)]
struct State {
    pos: usize,
    captures: Vec<Option<usize>>,
    open: Vec<(usize, usize)>,
    counters: Vec<Counter>,
    atomic: Vec<usize>,
}
#[derive(Clone, Copy)]
pub(super) struct Capture {
    start: usize,
    end: usize,
    previous: Option<usize>,
}
impl State {
    fn capture(&mut self, slot: usize, start: usize, end: usize, budget: &mut Budget) {
        let index = budget.captures.len();
        budget.captures.push(Capture {
            start,
            end,
            previous: self.captures[slot],
        });
        self.captures[slot] = Some(index);
    }
    fn matched(&self, slot: usize, budget: &Budget) -> bool {
        self.captures[slot].is_some_and(|index| {
            let c = budget.captures[index];
            // .NET's unmatched marker can also be authored by an overlapping
            // lookaround transfer. Keep its conditional semantics; never use
            // a negative capture length as an unchecked slice/allocation.
            c.end.checked_add(2) != Some(c.start)
        })
    }
}
impl Program {
    pub(super) fn is_match(&self, text: &str, slots: usize) -> Result<bool> {
        // Bound the allocation as well as the later execution. Never collect an
        // arbitrarily large untrusted manifest name before checking its size.
        let input: Vec<u16> = text.encode_utf16().take(32769).collect();
        ensure!(
            input.len() <= 32768,
            "Regex input exceeds resource-path budget"
        );
        let mut budget = Budget {
            steps: 100_000,
            deadline: Instant::now() + Duration::from_millis(30),
            captures: Vec::new(),
        };
        for pos in 0..=input.len() {
            let state = State {
                pos,
                captures: vec![None; slots],
                open: vec![],
                counters: vec![Counter::default(); self.counters],
                atomic: vec![],
            };
            if self.execute(&input, state, &mut budget)?.is_some() {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn execute(
        &self,
        input: &[u16],
        mut state: State,
        budget: &mut Budget,
    ) -> Result<Option<State>> {
        let mut pc = 0;
        let mut stack: Vec<(usize, State)> = vec![];
        loop {
            budget.step()?;
            let mut accepted = true;
            let current = if self.reverse {
                state.pos.checked_sub(1)
            } else {
                (state.pos < input.len()).then_some(state.pos)
            };
            match &self.ops[pc] {
                Op::Accept => return Ok(Some(state)),
                Op::Jump(next) => {
                    pc = *next;
                    continue;
                }
                Op::Split(first, second) => {
                    stack.push((*second, state.clone()));
                    ensure!(stack.len() <= 4096, "Regex pending-state budget reached");
                    pc = *first;
                    continue;
                }
                Op::Lit(c, i) => {
                    accepted = current.is_some_and(|p| {
                        if *i {
                            equivalent(input[p]) == equivalent(*c)
                        } else {
                            input[p] == *c
                        }
                    });
                    if accepted {
                        if self.reverse {
                            state.pos -= 1
                        } else {
                            state.pos += 1
                        }
                    }
                }
                Op::Any(all) => {
                    accepted = current.is_some_and(|p| *all || input[p] != 10);
                    if accepted {
                        if self.reverse {
                            state.pos -= 1
                        } else {
                            state.pos += 1
                        }
                    }
                }
                Op::Class(s) => {
                    accepted = current.is_some_and(|p| s.has(input[p]));
                    if accepted {
                        if self.reverse {
                            state.pos -= 1
                        } else {
                            state.pos += 1
                        }
                    }
                }
                Op::Anchor(a) => {
                    accepted = match a {
                        Anchor::Start => state.pos == 0,
                        Anchor::End => state.pos == input.len(),
                        Anchor::EndFinalLf => {
                            state.pos == input.len()
                                || state.pos + 1 == input.len() && input[state.pos] == 10
                        }
                        Anchor::StartLine => state.pos == 0 || input[state.pos - 1] == 10,
                        Anchor::EndLine => state.pos == input.len() || input[state.pos] == 10,
                        Anchor::Boundary(expected) => {
                            let left = state.pos > 0 && word(input[state.pos - 1]);
                            let right = state.pos < input.len() && word(input[state.pos]);
                            (left != right) == *expected
                        }
                    };
                }
                Op::Open(slot) => state.open.push((*slot, state.pos)),
                Op::Close(slot) => {
                    let (opened, start) =
                        state.open.pop().context("Missing regex capture start")?;
                    ensure!(opened == *slot, "Unbalanced regex capture");
                    state.capture(*slot, start.min(state.pos), start.max(state.pos), budget);
                }
                Op::Balance(destination, source) => {
                    let (opened, position) = state
                        .open
                        .pop()
                        .context("Missing balancing capture start")?;
                    ensure!(opened == *source, "Unbalanced regex subtraction");
                    accepted = state.matched(*source, budget);
                    if accepted {
                        let old = budget.captures[state.captures[*source].unwrap()];
                        ensure!(old.end >= old.start, "Invalid balanced capture interval");
                        state.captures[*source] = old.previous;
                        if let Some(slot) = destination {
                            // Transfer the innermost gap/overlap interval,
                            // following .NET RegexRunner.TransferCapture.
                            let (mut start, mut end) =
                                (position.min(state.pos), position.max(state.pos));
                            if start >= old.end {
                                end = start;
                                start = old.end;
                            } else if end <= old.start {
                                start = old.start;
                            } else {
                                end = end.min(old.end);
                                start = start.max(old.start);
                            }
                            state.capture(*slot, start, end, budget);
                        }
                    }
                }
                Op::Backref(slot, i) => {
                    accepted = false;
                    if state.matched(*slot, budget) {
                        let capture = budget.captures[state.captures[*slot].unwrap()];
                        let (a, b) = (capture.start, capture.end);
                        let len = b
                            .checked_sub(a)
                            .context("Invalid balanced backreference interval")?;
                        let start = if self.reverse {
                            state.pos.checked_sub(len)
                        } else {
                            Some(state.pos)
                        };
                        if let Some(start) = start.filter(|s| *s + len <= input.len()) {
                            accepted =
                                input[a..b]
                                    .iter()
                                    .zip(&input[start..start + len])
                                    .all(|(a, b)| {
                                        if *i {
                                            equivalent(*a) == equivalent(*b)
                                        } else {
                                            a == b
                                        }
                                    });
                            if accepted {
                                state.pos = if self.reverse { start } else { start + len };
                            }
                        }
                    }
                }
                Op::Look(program, positive) => {
                    let original = state.pos;
                    let found = program.execute(input, state.clone(), budget)?;
                    if *positive {
                        if let Some(mut found) = found {
                            found.pos = original;
                            state = found
                        } else {
                            accepted = false
                        }
                    } else {
                        accepted = found.is_none()
                    }
                }
                Op::AtomicStart => state.atomic.push(stack.len()),
                Op::AtomicEnd => {
                    let depth = state.atomic.pop().context("Missing atomic marker")?;
                    stack.truncate(depth);
                }
                Op::Condition(slot, yes, no) => {
                    pc = if state.matched(*slot, budget) {
                        *yes
                    } else {
                        *no
                    };
                    continue;
                }
                Op::AssertCondition(program, yes, no) => {
                    if let Some(found) = program.execute(input, state.clone(), budget)? {
                        state = found;
                        pc = *yes
                    } else {
                        pc = *no
                    }
                    continue;
                }
                Op::Reset(id) => state.counters[*id] = Counter::default(),
                Op::Check {
                    id,
                    lo,
                    hi,
                    greedy,
                    body,
                    after,
                } => {
                    let count = state.counters[*id].count;
                    if count >= *hi {
                        pc = *after;
                        continue;
                    }
                    if count < *lo {
                        state.counters[*id].previous = state.pos;
                        pc = *body;
                        continue;
                    }
                    if *greedy {
                        stack.push((*after, state.clone()));
                        state.counters[*id].previous = state.pos;
                        pc = *body
                    } else {
                        let mut later = state.clone();
                        later.counters[*id].previous = state.pos;
                        stack.push((*body, later));
                        pc = *after
                    }
                    ensure!(stack.len() <= 4096, "Regex pending-state budget reached");
                    continue;
                }
                Op::RepeatEnd {
                    id,
                    lo,
                    check,
                    after,
                } => {
                    let counter = &mut state.counters[*id];
                    counter.count += 1;
                    pc = if counter.count >= *lo && counter.previous == state.pos {
                        *after
                    } else {
                        *check
                    };
                    continue;
                }
            }
            if accepted {
                pc += 1
            } else if let Some((next, saved)) = stack.pop() {
                pc = next;
                state = saved
            } else {
                return Ok(None);
            }
        }
    }
}

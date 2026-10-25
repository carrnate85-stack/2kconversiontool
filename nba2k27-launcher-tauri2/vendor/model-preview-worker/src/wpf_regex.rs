//! App-local UTF16/invariant regex matching, backed by frozen reference facts.
//! No reference helper is invoked by this implementation.
use anyhow::{Context, Result, bail, ensure};
use std::{
    collections::HashMap,
    sync::LazyLock,
    time::{Duration, Instant},
};
const EQUIV: &[u8] = include_bytes!("../Data/wpf-regex-equivalence.bin");
const INFO: &[u8] = include_bytes!("../Data/wpf-regex-character-info.bin");
#[path = "wpf_regex_vm.rs"]
mod vm;
fn equivalent(c: u16) -> u16 {
    let p = c as usize * 2;
    u16::from_le_bytes([EQUIV[p], EQUIV[p + 1]])
}
fn word(c: u16) -> bool {
    INFO[c as usize] & 128 != 0
}
#[derive(Clone)]
struct Set(Box<[u64]>);
impl Set {
    fn new() -> Self {
        Self(vec![0; 1024].into_boxed_slice())
    }
    fn add(&mut self, c: u16) {
        self.0[c as usize / 64] |= 1u64 << (c % 64);
    }
    fn has(&self, c: u16) -> bool {
        self.0[c as usize / 64] & (1u64 << (c % 64)) != 0
    }
    fn union(&mut self, other: &Self) {
        for (a, b) in self.0.iter_mut().zip(other.0.iter()) {
            *a |= *b;
        }
    }
    fn subtract(&mut self, other: &Self) {
        for (a, b) in self.0.iter_mut().zip(other.0.iter()) {
            *a &= !*b;
        }
    }
    fn invert(&mut self) {
        for a in self.0.iter_mut() {
            *a = !*a;
        }
    }
    fn insensitive(&mut self) {
        let mut classes = Self::new();
        for c in 0..=u16::MAX {
            if self.has(c) {
                classes.add(equivalent(c));
            }
        }
        for c in 0..=u16::MAX {
            if classes.has(equivalent(c)) {
                self.add(c);
            }
        }
    }
}
#[derive(Clone, Copy)]
struct Flags {
    i: bool,
    m: bool,
    s: bool,
    x: bool,
    n: bool,
}
impl Default for Flags {
    fn default() -> Self {
        Self {
            i: true,
            m: false,
            s: false,
            x: false,
            n: false,
        }
    }
}
#[derive(Clone, Copy)]
enum Anchor {
    Start,
    End,
    EndFinalLf,
    StartLine,
    EndLine,
    Boundary(bool),
}
enum Node {
    Empty,
    Directive,
    Lit(u16, bool),
    Any(bool),
    Class(Set),
    Anchor(Anchor),
    Seq(Vec<Node>),
    Alt(Vec<Node>),
    Capture(usize, Box<Node>),
    RawBalance(Option<usize>, String, Box<Node>),
    Balance(Option<usize>, usize, Box<Node>),
    Repeat(Box<Node>, usize, usize, bool),
    Look(Box<Node>, bool, bool),
    Atomic(Box<Node>),
    RawRef(String, bool, bool),
    Backref(usize, bool),
    RawCondition(String, Box<Node>, Box<Node>, Box<Node>),
    Condition(usize, Box<Node>, Box<Node>),
    AssertCondition(Box<Node>, Box<Node>, Box<Node>),
}
struct Parser {
    src: Vec<u16>,
    p: usize,
    flags: Flags,
    captures: Vec<Option<String>>,
    depth: usize,
    fast_safe: bool,
}
pub struct Regex {
    root: vm::Program,
    slots: usize,
    ascii_possible: bool,
    ascii_fast_safe: bool,
}
impl Regex {
    pub fn new(pattern: &str) -> Result<Self> {
        ensure!(
            pattern.encode_utf16().count() <= 256,
            "Regex patterns are limited to 256 characters"
        );
        let mut p = Parser {
            src: pattern.encode_utf16().collect(),
            p: 0,
            flags: Flags::default(),
            captures: Vec::new(),
            depth: 0,
            fast_safe: true,
        };
        let mut root = p.expression()?;
        ensure!(p.p == p.src.len(), "Unexpected closing parenthesis");
        let (physical, numbers, names, slots) = capture_slots(&p.captures)?;
        link(&mut root, &physical, &numbers, &names)?;
        let ascii_possible = can_match_ascii(&root);
        let ascii_fast_safe = pattern.is_ascii()
            && p.fast_safe
            && !pattern.contains("-[")
            && !pattern.contains("(?(")
            && !p.captures.iter().any(Option::is_some)
            && !pattern.contains("\\p")
            && !pattern.contains("\\P");
        Ok(Self {
            root: vm::compile(&root)?,
            slots,
            ascii_possible,
            ascii_fast_safe,
        })
    }
    pub fn ascii_possible(&self) -> bool {
        self.ascii_possible
    }
    pub fn ascii_fast_safe(&self) -> bool {
        self.ascii_fast_safe
    }
    pub fn is_match(&self, text: &str) -> Result<bool> {
        self.root.is_match(text, self.slots)
    }
}
fn can_match_ascii(node: &Node) -> bool {
    match node {
        Node::Lit(c, i) => {
            if *i {
                (0..128).any(|x| equivalent(x) == equivalent(*c))
            } else {
                *c < 128
            }
        }
        Node::Class(s) => (0..128).any(|x| s.has(x)),
        Node::Seq(children) => children.iter().all(can_match_ascii),
        Node::Alt(children) => children.iter().any(can_match_ascii),
        Node::Capture(_, child) | Node::Balance(_, _, child) | Node::Atomic(child) => {
            can_match_ascii(child)
        }
        Node::Repeat(child, lo, ..) => *lo == 0 || can_match_ascii(child),
        Node::Look(child, positive, _) => !*positive || can_match_ascii(child),
        _ => true,
    }
}
fn capture_slots(
    specs: &[Option<String>],
) -> Result<(
    Vec<usize>,
    HashMap<u32, usize>,
    HashMap<String, usize>,
    usize,
)> {
    let mut numbers = HashMap::new();
    numbers.insert(0, 0);
    let mut names = HashMap::new();
    let mut slots = 1;
    for name in specs
        .iter()
        .flatten()
        .filter(|n| n.bytes().all(|c| c.is_ascii_digit()))
    {
        let num: u32 = name.parse()?;
        ensure!(num <= i32::MAX as u32, "Capture number exceeds Int32 range");
        ensure!(num > 0, "Capture zero cannot be declared");
        // .NET's declaration scan does not reserve a leading-zero number.
        // Parsing can still alias an independently declared numeric slot.
        if name.starts_with('0') {
            continue;
        }
        if !numbers.contains_key(&num) {
            numbers.insert(num, slots);
            slots += 1;
        }
    }
    let mut physical = vec![0; specs.len()];
    let mut next = 1u32;
    for (id, name) in specs.iter().enumerate().filter(|(_, n)| n.is_none()) {
        while numbers.contains_key(&next) {
            next += 1;
        }
        numbers.insert(next, slots);
        physical[id] = slots;
        slots += 1;
        next += 1;
        let _ = name;
    }
    for (id, name) in specs
        .iter()
        .enumerate()
        .filter_map(|(i, n)| n.as_ref().map(|n| (i, n)))
    {
        if name.starts_with('0') && name.bytes().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let slot = if name.bytes().all(|c| c.is_ascii_digit()) {
            numbers[&name.parse::<u32>()?]
        } else if let Some(&slot) = names.get(name) {
            slot
        } else {
            while numbers.contains_key(&next) {
                next += 1;
            }
            numbers.insert(next, slots);
            names.insert(name.clone(), slots);
            slots += 1;
            next += 1;
            slots - 1
        };
        physical[id] = slot;
    }
    for (id, name) in specs
        .iter()
        .enumerate()
        .filter_map(|(i, n)| n.as_ref().map(|n| (i, n)))
    {
        if name.starts_with('0') && name.bytes().all(|c| c.is_ascii_digit()) {
            physical[id] = *numbers
                .get(&name.parse::<u32>()?)
                .context("Undefined numeric capture name")?;
        }
    }
    Ok((physical, numbers, names, slots))
}
fn link(
    node: &mut Node,
    physical: &[usize],
    numbers: &HashMap<u32, usize>,
    names: &HashMap<String, usize>,
) -> Result<()> {
    match node {
        Node::RawBalance(destination, source, child) => {
            let source_slot = if source.bytes().all(|c| c.is_ascii_digit()) {
                let number = source.parse::<u32>()?;
                ensure!(
                    number <= i32::MAX as u32,
                    "Capture reference exceeds Int32 range"
                );
                numbers.get(&number).copied()
            } else {
                names.get(source).copied()
            }
            .context("Undefined balancing capture")?;
            link(child, physical, numbers, names)?;
            let destination = destination.map(|slot| physical[slot]);
            let child = std::mem::replace(child, Box::new(Node::Empty));
            *node = Node::Balance(destination, source_slot, child);
        }
        Node::Capture(slot, child) => {
            *slot = physical[*slot];
            link(child, physical, numbers, names)?;
        }
        Node::Seq(children) | Node::Alt(children) => {
            for child in children {
                link(child, physical, numbers, names)?;
            }
        }
        Node::Repeat(child, ..) | Node::Look(child, ..) | Node::Atomic(child) => {
            link(child, physical, numbers, names)?
        }
        Node::RawRef(value, casei, named) => {
            if value.bytes().all(|c| c.is_ascii_digit()) {
                ensure!(
                    value.parse::<u32>().is_ok_and(|n| n <= i32::MAX as u32),
                    "Capture reference exceeds Int32 range"
                );
            }
            let slot = if *named && !value.bytes().all(|c| c.is_ascii_digit()) {
                names.get(value).copied()
            } else {
                value
                    .parse::<u32>()
                    .ok()
                    .and_then(|n| numbers.get(&n).copied())
            };
            if let Some(slot) = slot {
                *node = Node::Backref(slot, *casei)
            } else if !*named && value.len() >= 2 {
                let digits: Vec<u16> = value.encode_utf16().collect();
                let mut end = 0;
                let mut code = 0;
                while end < digits.len().min(3) && (48..=55).contains(&digits[end]) {
                    code = code * 8 + digits[end] - 48;
                    end += 1;
                }
                ensure!(end > 0, "Undefined backreference");
                let mut children = vec![Node::Lit(code, *casei)];
                children.extend(digits[end..].iter().map(|c| Node::Lit(*c, *casei)));
                *node = Node::Seq(children);
            } else {
                bail!("Undefined capture reference")
            }
        }
        Node::AssertCondition(condition, yes, no) => {
            link(condition, physical, numbers, names)?;
            link(yes, physical, numbers, names)?;
            link(no, physical, numbers, names)?;
        }
        Node::RawCondition(value, condition, yes, no) => {
            let slot = value
                .parse::<u32>()
                .ok()
                .and_then(|n| numbers.get(&n).copied())
                .or_else(|| names.get(value).copied());
            link(yes, physical, numbers, names)?;
            link(no, physical, numbers, names)?;
            let yes = std::mem::replace(yes, Box::new(Node::Empty));
            let no = std::mem::replace(no, Box::new(Node::Empty));
            *node = if let Some(slot) = slot {
                Node::Condition(slot, yes, no)
            } else {
                ensure!(
                    !value.bytes().all(|c| c.is_ascii_digit()),
                    "Undefined conditional capture"
                );
                link(condition, physical, numbers, names)?;
                let condition = std::mem::replace(condition, Box::new(Node::Empty));
                Node::AssertCondition(Box::new(Node::Look(condition, true, false)), yes, no)
            };
        }
        _ => {}
    }
    Ok(())
}
impl Parser {
    fn peek(&self) -> Option<u16> {
        self.src.get(self.p).copied()
    }
    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c as u16) {
            self.p += 1;
            true
        } else {
            false
        }
    }
    fn require(&mut self, c: u8) -> Result<()> {
        ensure!(self.eat(c), "Expected '{}'", c as char);
        Ok(())
    }
    fn skip(&mut self) {
        if self.flags.x {
            loop {
                while self
                    .peek()
                    .is_some_and(|c| [9, 10, 12, 13, 32].contains(&c))
                {
                    self.p += 1;
                }
                if self.eat(b'#') {
                    while self.peek().is_some_and(|c| c != 10) {
                        self.p += 1;
                    }
                } else {
                    break;
                }
            }
        }
    }
    fn expression(&mut self) -> Result<Node> {
        let mut branches = vec![self.sequence()?];
        while self.eat(b'|') {
            branches.push(self.sequence()?)
        }
        Ok(if branches.len() == 1 {
            branches.pop().unwrap()
        } else {
            Node::Alt(branches)
        })
    }
    fn sequence(&mut self) -> Result<Node> {
        let mut children = Vec::new();
        loop {
            self.skip();
            if self
                .peek()
                .is_none_or(|c| c == b'|' as u16 || c == b')' as u16)
            {
                break;
            }
            let atom = self.atom()?;
            children.push(self.quantifier(atom)?);
        }
        Ok(match children.len() {
            0 => Node::Empty,
            1 => children.pop().unwrap(),
            _ => Node::Seq(children),
        })
    }
    fn atom(&mut self) -> Result<Node> {
        let c = self.peek().context("Expected expression")?;
        self.p += 1;
        match c {
            40 => self.group(),
            91 => {
                let set = self.class()?;
                Ok(Node::Class(set))
            }
            46 => Ok(Node::Any(self.flags.s)),
            94 => Ok(Node::Anchor(if self.flags.m {
                Anchor::StartLine
            } else {
                Anchor::Start
            })),
            36 => Ok(Node::Anchor(if self.flags.m {
                Anchor::EndLine
            } else {
                Anchor::EndFinalLf
            })),
            92 => self.escape(false),
            42 | 43 | 63 => bail!("Quantifier has no target"),
            _ => Ok(Node::Lit(c, self.flags.i)),
        }
    }
    fn group(&mut self) -> Result<Node> {
        self.depth += 1;
        ensure!(self.depth <= 128, "Regex group nesting exceeds budget");
        let before = self.flags;
        let mut capture = None;
        let mut balance = None;
        let mut kind = 0;
        if self.eat(b'?') {
            match self.peek() {
                Some(58) => {
                    self.p += 1;
                }
                Some(61) => {
                    self.p += 1;
                    kind = 1;
                }
                Some(33) => {
                    self.p += 1;
                    kind = 2;
                }
                Some(62) => {
                    self.p += 1;
                    kind = 5;
                }
                Some(35) => {
                    self.p += 1;
                    while self.peek().is_some_and(|c| c != 41) {
                        self.p += 1;
                    }
                    self.require(b')')?;
                    self.depth -= 1;
                    return Ok(Node::Directive);
                }
                Some(60) => {
                    self.p += 1;
                    if self.eat(b'=') {
                        kind = 3
                    } else if self.eat(b'!') {
                        kind = 4
                    } else {
                        let spec = self.raw_name(b'>')?;
                        (capture, balance) = self.capture_spec(spec)?;
                    }
                }
                Some(39) => {
                    self.p += 1;
                    let spec = self.raw_name(b'\'')?;
                    (capture, balance) = self.capture_spec(spec)?;
                }
                Some(40) => {
                    self.p += 1;
                    let condition = if self.peek() == Some(63) {
                        Some(self.group()?)
                    } else {
                        None
                    };
                    let value = if condition.is_none() {
                        let start = self.p;
                        let parsed = self.expression()?;
                        let text = String::from_utf16(&self.src[start..self.p])?;
                        self.require(b')')?;
                        Some((text, parsed))
                    } else {
                        None
                    };
                    let yes = self.sequence()?;
                    let no = if self.eat(b'|') {
                        self.sequence()?
                    } else {
                        Node::Empty
                    };
                    self.require(b')')?;
                    self.flags = before;
                    self.depth -= 1;
                    return Ok(if let Some(condition) = condition {
                        Node::AssertCondition(Box::new(condition), Box::new(yes), Box::new(no))
                    } else {
                        let (text, parsed) = value.unwrap();
                        Node::RawCondition(text, Box::new(parsed), Box::new(yes), Box::new(no))
                    });
                }
                _ => {
                    let mut enable = true;
                    let mut found = false;
                    while let Some(c) = self.peek() {
                        if c == 45 {
                            enable = false;
                            self.p += 1;
                            continue;
                        }
                        if c == 120 || c == 110 {
                            self.fast_safe = false;
                        }
                        let flag = match c {
                            105 => &mut self.flags.i,
                            109 => &mut self.flags.m,
                            115 => &mut self.flags.s,
                            120 => &mut self.flags.x,
                            110 => &mut self.flags.n,
                            _ => break,
                        };
                        *flag = enable;
                        found = true;
                        self.p += 1;
                    }
                    ensure!(found, "Unknown group construct");
                    if self.eat(b')') {
                        self.depth -= 1;
                        return Ok(Node::Directive);
                    }
                    self.require(b':')?;
                }
            }
        } else if !self.flags.n {
            capture = Some(None)
        }
        if let Some(Some(name)) = &capture {
            let units: Vec<_> = name.encode_utf16().collect();
            ensure!(units.iter().all(|c| word(*c)), "Invalid capture name");
            ensure!(
                units.iter().all(|c| (48..=57).contains(c)) || INFO[units[0] as usize] & 31 != 8,
                "Capture name starts with a digit"
            );
        }
        let id = capture.map(|name| {
            let id = self.captures.len();
            self.captures.push(name);
            id
        });
        let child = self.expression()?;
        self.require(b')')?;
        self.flags = before;
        self.depth -= 1;
        let child = Box::new(child);
        Ok(if let Some(source) = balance {
            Node::RawBalance(id, source, child)
        } else if let Some(id) = id {
            Node::Capture(id, child)
        } else {
            match kind {
                1 => Node::Look(child, true, false),
                2 => Node::Look(child, false, false),
                3 => Node::Look(child, true, true),
                4 => Node::Look(child, false, true),
                5 => Node::Atomic(child),
                _ => *child,
            }
        })
    }
    fn name(&mut self, end: u8) -> Result<String> {
        let name = self.raw_name(end)?;
        ensure!(!name.contains('-'), "Invalid capture reference");
        Ok(name)
    }
    fn capture_spec(&mut self, spec: String) -> Result<(Option<Option<String>>, Option<String>)> {
        if let Some((destination, source)) = spec.split_once('-') {
            self.fast_safe = false;
            let units: Vec<_> = source.encode_utf16().collect();
            ensure!(
                !units.is_empty() && units.iter().all(|c| word(*c)),
                "Invalid balancing capture name"
            );
            ensure!(
                units.iter().all(|c| (48..=57).contains(c)) || INFO[units[0] as usize] & 31 != 8,
                "Capture name starts with a digit"
            );
            Ok((
                (!destination.is_empty()).then(|| Some(destination.into())),
                Some(source.into()),
            ))
        } else {
            Ok((Some(Some(spec)), None))
        }
    }
    fn raw_name(&mut self, end: u8) -> Result<String> {
        let start = self.p;
        while self.peek().is_some_and(|c| c != end as u16) {
            self.p += 1;
        }
        ensure!(self.p > start, "Empty capture name");
        let name = String::from_utf16(&self.src[start..self.p])?;
        self.require(end)?;
        Ok(name)
    }
    fn number(&mut self) -> Result<usize> {
        let start = self.p;
        while self.peek().is_some_and(|c| (48..=57).contains(&c)) {
            self.p += 1;
        }
        ensure!(self.p > start, "Expected repeat count");
        let value = String::from_utf16(&self.src[start..self.p])?.parse::<usize>()?;
        ensure!(
            value <= i32::MAX as usize,
            "Repeat count exceeds Int32 range"
        );
        Ok(value)
    }
    fn quantifier(&mut self, child: Node) -> Result<Node> {
        self.skip();
        let (lo, hi) = if self.eat(b'*') {
            (0, usize::MAX)
        } else if self.eat(b'+') {
            (1, usize::MAX)
        } else if self.eat(b'?') {
            (0, 1)
        } else if self.peek() == Some(123) {
            let start = self.p;
            self.p += 1;
            if !self.peek().is_some_and(|c| (48..=57).contains(&c)) {
                self.p = start;
                return Ok(child);
            }
            let lo = self.number()?;
            let hi = if self.eat(b',') {
                if self.peek().is_some_and(|c| (48..=57).contains(&c)) {
                    self.number()?
                } else {
                    usize::MAX
                }
            } else {
                lo
            };
            if !self.eat(b'}') {
                self.p = start;
                return Ok(child);
            }
            ensure!(lo <= hi, "Repeat minimum exceeds maximum");
            (lo, hi)
        } else {
            return Ok(child);
        };
        let greedy = !self.eat(b'?');
        ensure!(
            !matches!(child, Node::Directive),
            "Quantifier follows an option/comment directive"
        );
        ensure!(
            !self.peek().is_some_and(|c| [42, 43, 63].contains(&c)),
            "Nested quantifier"
        );
        Ok(Node::Repeat(Box::new(child), lo, hi, greedy))
    }
    fn hex(&mut self, count: usize) -> Result<u16> {
        let mut value = 0;
        for _ in 0..count {
            let c = self.peek().context("Incomplete hex escape")?;
            let digit = char::from_u32(c as u32)
                .and_then(|c| c.to_digit(16))
                .context("Invalid hex escape")?;
            value = value * 16 + digit as u16;
            self.p += 1;
        }
        Ok(value)
    }
    fn escape(&mut self, in_class: bool) -> Result<Node> {
        let c = self.peek().context("Trailing escape")?;
        self.p += 1;
        if [100, 68, 119, 87, 115, 83, 112, 80].contains(&c) {
            let mut set = if c == 112 || c == 80 {
                self.category()?
            } else {
                let mask = match c {
                    100 | 68 => 0,
                    119 | 87 => 32,
                    _ => 64,
                };
                let mut s = Set::new();
                for unit in 0..=u16::MAX {
                    if if mask == 0 {
                        INFO[unit as usize] & 31 == 8
                    } else {
                        INFO[unit as usize] & mask != 0
                    } {
                        s.add(unit)
                    }
                }
                s
            };
            if self.flags.i {
                set.insensitive()
            }
            if [68, 87, 83, 80].contains(&c) {
                set.invert()
            }
            return Ok(Node::Class(set));
        }
        if !in_class {
            let anchor = match c {
                65 | 71 => Some(Anchor::Start),
                122 => Some(Anchor::End),
                90 => Some(Anchor::EndFinalLf),
                98 => Some(Anchor::Boundary(true)),
                66 => Some(Anchor::Boundary(false)),
                _ => None,
            };
            if let Some(a) = anchor {
                return Ok(Node::Anchor(a));
            }
            if c == 107 {
                let end = if self.eat(b'<') {
                    b'>'
                } else {
                    self.require(b'\'')?;
                    b'\''
                };
                let name = self.name(end)?;
                return Ok(Node::RawRef(name, self.flags.i, true));
            }
            if (49..=57).contains(&c) {
                let mut digits = vec![c];
                while self.peek().is_some_and(|c| (48..=57).contains(&c)) {
                    digits.push(self.peek().unwrap());
                    self.p += 1;
                }
                return Ok(Node::RawRef(
                    String::from_utf16(&digits)?,
                    self.flags.i,
                    false,
                ));
            }
        }
        let value = match c {
            97 => 7,
            98 if in_class => 8,
            101 => 27,
            102 => 12,
            110 => 10,
            114 => 13,
            116 => 9,
            118 => 11,
            120 => self.hex(2)?,
            117 => self.hex(4)?,
            99 => {
                let c = self.peek().context("Incomplete control escape")?;
                self.p += 1;
                let c = if (97..=122).contains(&c) { c - 32 } else { c };
                ensure!((64..=95).contains(&c), "Invalid control escape");
                c - 64
            }
            48 => {
                let mut value = 0;
                for _ in 0..2 {
                    if let Some(c) = self.peek().filter(|c| (48..=55).contains(c)) {
                        value = value * 8 + c - 48;
                        self.p += 1;
                    } else {
                        break;
                    }
                }
                value
            }
            _ => {
                ensure!(!word(c), "Unrecognized escape");
                c
            }
        };
        // An ASCII source spelling can still denote a non-ASCII character.
        // Rust's Unicode folding differs from invariant .NET folding (e.g.
        // [^\u017f] must allow ASCII s/S). Such escapes require the native VM,
        // including inside classes/negation, not the ASCII-name fast path.
        if value >= 128 {
            self.fast_safe = false;
        }
        Ok(Node::Lit(value, self.flags.i))
    }
    fn category(&mut self) -> Result<Set> {
        self.require(b'{')?;
        let start = self.p;
        while self.peek().is_some_and(|c| c != 125) {
            self.p += 1;
        }
        let name = String::from_utf16(&self.src[start..self.p])?;
        self.require(b'}')?;
        const CATEGORIES: [&str; 30] = [
            "Lu", "Ll", "Lt", "Lm", "Lo", "Mn", "Mc", "Me", "Nd", "Nl", "No", "Zs", "Zl", "Zp",
            "Cc", "Cf", "Cs", "Co", "Pc", "Pd", "Ps", "Pe", "Pi", "Pf", "Po", "Sm", "Sc", "Sk",
            "So", "Cn",
        ];
        let code = CATEGORIES.iter().position(|n| *n == name);
        let mut set = Set::new();
        if code.is_some() || ["L", "M", "N", "Z", "C", "P", "S"].contains(&name.as_str()) {
            for c in 0..=u16::MAX {
                let k = INFO[c as usize] & 31;
                let yes = if let Some(code) = code {
                    if self.flags.i && code <= 2 {
                        k <= 2
                    } else {
                        k as usize == code
                    }
                } else {
                    match name.as_str() {
                        "L" => k <= 4,
                        "M" => (5..=7).contains(&k),
                        "N" => (8..=10).contains(&k),
                        "Z" => (11..=13).contains(&k),
                        "C" => (14..=17).contains(&k) || k == 29,
                        "P" => (18..=24).contains(&k),
                        "S" => (25..=28).contains(&k),
                        _ => false,
                    }
                };
                if yes {
                    set.add(c)
                }
            }
        } else {
            #[derive(serde::Deserialize)]
            struct Block {
                name: String,
                ranges: Vec<u16>,
            }
            static BLOCKS: LazyLock<Vec<Block>> = LazyLock::new(|| {
                serde_json::from_str(include_str!("../Data/wpf-regex-blocks.json"))
                    .expect("Reference blocks")
            });
            let block = BLOCKS
                .iter()
                .find(|b| b.name == name)
                .context("Unknown Unicode category/block")?;
            for pair in block.ranges.chunks(2) {
                let start = pair[0] as usize;
                let end = pair.get(1).map_or(65536, |x| *x as usize);
                for c in start..end {
                    set.add(c as u16)
                }
            }
        }
        Ok(set)
    }
    fn class_piece(&mut self) -> Result<(Set, Option<u16>)> {
        let node = if self.eat(b'\\') {
            self.escape(true)?
        } else {
            let c = self.peek().context("Unterminated class")?;
            self.p += 1;
            Node::Lit(c, self.flags.i)
        };
        match node {
            Node::Lit(c, _) => {
                let mut set = Set::new();
                set.add(c);
                Ok((set, Some(c)))
            }
            Node::Class(s) => Ok((s, None)),
            _ => bail!("Invalid class member"),
        }
    }
    fn class(&mut self) -> Result<Set> {
        let negate = self.eat(b'^');
        let mut set = Set::new();
        let mut first = true;
        loop {
            ensure!(self.peek().is_some(), "Unterminated character class");
            if !first && self.eat(b']') {
                break;
            }
            let (mut piece, unit) = self.class_piece()?;
            first = false;
            if self.peek() == Some(45)
                && self
                    .src
                    .get(self.p + 1)
                    .is_some_and(|c| *c != 93 && *c != 91)
            {
                self.p += 1;
                let (_, end) = self.class_piece()?;
                let start = unit.context("Class escape cannot start a range")?;
                let end = end.context("Class escape cannot end a range")?;
                ensure!(start <= end, "Reversed character range");
                piece = Set::new();
                for c in start..=end {
                    piece.add(c)
                }
            }
            set.union(&piece);
            if self.peek() == Some(45) && self.src.get(self.p + 1) == Some(&91) {
                self.p += 2;
                if self.flags.i {
                    set.insensitive()
                }
                if negate {
                    set.invert()
                }
                let sub = self.class()?;
                set.subtract(&sub);
                self.require(b']')?;
                return Ok(set);
            }
        }
        if self.flags.i {
            set.insensitive()
        }
        if negate {
            set.invert()
        }
        Ok(set)
    }
}
struct Budget {
    steps: usize,
    deadline: Instant,
    // Append-only histories share immutable links across saved VM states.
    // Each entry requires a budgeted Close/Balance instruction; no recursive
    // linked allocation/destructor or full-history copy on backtracking.
    captures: Vec<vm::Capture>,
}
impl Budget {
    fn step(&mut self) -> Result<()> {
        ensure!(self.steps > 0, "Regex backtracking limit reached");
        self.steps -= 1;
        if self.steps % 128 == 0 {
            ensure!(Instant::now() <= self.deadline, "Regex match exceeded 30ms");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_facts_are_pinned_and_idempotent() {
        use sha2::{Digest, Sha256};
        assert_eq!(EQUIV.len(), 131072);
        assert_eq!(INFO.len(), 65536);
        assert_eq!(
            format!("{:X}", Sha256::digest(EQUIV)),
            "7AD7D39129A405264DAED649C889ABDD35AA5976460102343E9B327D00B0AE6C"
        );
        assert_eq!(
            format!("{:X}", Sha256::digest(INFO)),
            "015D0A8EB56153BA1FBE139B5C78EC28D643E1C56E307C0A79A1090ED0E9475D"
        );
        assert_eq!(
            format!(
                "{:X}",
                Sha256::digest(include_bytes!("../Data/wpf-regex-blocks.json"))
            ),
            "8BC0FE25F37CDCA4E1BD7665AFF00A6D31EDB68701C0F34872B5F63DCAECE1AD"
        );
        for c in 0..=u16::MAX {
            assert_eq!(equivalent(equivalent(c)), equivalent(c));
            assert!(equivalent(c) <= c);
            assert!(INFO[c as usize] & 31 <= 29);
        }
    }
    #[test]
    fn greedy_unicode_success_is_lazy_not_exhaustive() {
        let text = format!("{}b", "Σ".repeat(32));
        assert!(Regex::new("^(Σ|ΣΣ)*b$").unwrap().is_match(&text).unwrap());
        assert!(Regex::new("^(?:Σ?)*b$").unwrap().is_match(&text).unwrap());
        assert!(
            Regex::new("^Σ*$")
                .unwrap()
                .is_match(&"Σ".repeat(1000))
                .unwrap()
        );
        assert!(!Regex::new("^(?>Σ|ΣΣ)b$").unwrap().is_match("ΣΣb").unwrap());
        assert!(
            Regex::new("^(?=(Σ+))Σ*b\\1$")
                .unwrap()
                .is_match("ΣΣbΣΣ")
                .unwrap()
        );
    }
    #[test]
    fn reference_regex_matrix() {
        let document: serde_json::Value = serde_json::from_str(include_str!(
            "../Tests/fixtures/wpf-regex-routing-cases.json"
        ))
        .unwrap();
        let mut cache: HashMap<String, Result<Regex, String>> = HashMap::new();
        let mut failures = vec![];
        for case in document["cases"].as_array().unwrap() {
            let pattern = case["pattern"].as_str().unwrap();
            let parsed = cache
                .entry(pattern.into())
                .or_insert_with(|| Regex::new(pattern).map_err(|e| e.to_string()));
            if case["compileError"] == true {
                if parsed.is_ok() {
                    failures.push(
                        serde_json::json!({"pattern":pattern,"unexpectedCompileSuccess":true}),
                    );
                }
                continue;
            }
            let input = case["input"].as_str().unwrap();
            let result = match parsed {
                Ok(regex) => regex.is_match(input).map_err(|e| e.to_string()),
                Err(e) => Err(e.clone()),
            };
            if result.as_ref().ok().copied() != case["expected"].as_bool() {
                failures.push(serde_json::json!({"pattern":pattern,"input":input,"expected":case["expected"],"result":format!("{result:?}")}));
            }
        }
        assert!(
            failures.is_empty(),
            "{} failures; first 24: {}",
            failures.len(),
            serde_json::to_string_pretty(&failures[..failures.len().min(24)]).unwrap()
        );
    }
    #[test]
    fn native_limits_are_explicit() {
        assert!(Regex::new(&"a".repeat(257)).is_err());
        assert!(
            Regex::new("(a+)+$")
                .unwrap()
                .is_match(&format!("{}!", "a".repeat(128)))
                .is_err()
        );
        let regex = Regex::new(".").unwrap();
        for input in ["a".repeat(32769), "😀".repeat(16385)] {
            assert!(
                regex
                    .is_match(&input)
                    .unwrap_err()
                    .to_string()
                    .contains("resource-path budget")
            );
        }
    }
    #[test]
    fn balancing_reference_matrix() {
        use sha2::{Digest, Sha256};
        let bytes = include_bytes!("../Tests/fixtures/wpf-regex-balancing-cases.json");
        assert_eq!(
            format!("{:X}", Sha256::digest(bytes)),
            "C90AD6C2897D4E4F93A268BD07F1409920BFD1A2BDE1C2934E4681D9023E640D"
        );
        let fixture: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        let mut cache: HashMap<String, Result<Regex, String>> = HashMap::new();
        for case in fixture["cases"].as_array().unwrap() {
            let pattern = case["pattern"].as_str().unwrap();
            let compiled = cache
                .entry(pattern.into())
                .or_insert_with(|| Regex::new(pattern).map_err(|e| e.to_string()));
            if case["compileError"] == true {
                assert!(compiled.is_err(), "{pattern}");
                continue;
            }
            let input = case["input"].as_str().unwrap();
            let regex = compiled
                .as_ref()
                .unwrap_or_else(|e| panic!("{pattern}: {e}"));
            let result = regex.is_match(input);
            if case["runtimeError"].is_string() {
                // The reference has an unsafe negative interval on this
                // pathological overlap. Require our specific safe rejection,
                // not an unrelated timeout/backtracking error counted as parity.
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("Invalid balanced backreference interval"),
                    "{pattern}: {input:?}"
                );
            } else {
                assert_eq!(
                    result.unwrap_or_else(|e| panic!("{pattern}: {input:?}: {e}")),
                    case["expected"].as_bool().unwrap(),
                    "{pattern}: {input:?}"
                );
            }
            assert!(
                !regex.ascii_fast_safe(),
                "Balancing must use its native capture history: {pattern}"
            );
        }
    }
    #[test]
    fn balancing_history_keeps_existing_execution_limits() {
        let regex = Regex::new("^(?<N>a)+(?<-N>b)+(?(N)(?!))$").unwrap();
        assert!(
            regex
                .is_match(&format!("{}{}", "a".repeat(1000), "b".repeat(1000)))
                .unwrap()
        );
        assert!(regex.is_match(&"a".repeat(6000)).is_err());
    }
    #[test]
    fn escaped_unicode_never_uses_ascii_case_folding() {
        for pattern in [r"[^\u017f]", r"(?i:[^\u017f])", r"[\u0000-\uffff]"] {
            let regex = Regex::new(pattern).unwrap();
            assert!(!regex.ascii_fast_safe(), "{pattern}");
            assert!(regex.is_match("S").unwrap());
        }
        assert!(Regex::new(r"png\d+\.iff$").unwrap().ascii_fast_safe());
    }
}

//! Hybrid logical clock whose string form sorts in causal order.

use crate::{Error, Result};
use std::time::{SystemTime, UNIX_EPOCH};

const MS_DIGITS: usize = 13;
const COUNTER_DIGITS: usize = 4;
const COUNTER_MAX: u32 = 0xffff;
const NODE_DIGITS: usize = 16;

/// Hybrid logical clock for one node. Stamps look like `1763683200000-0000-9f2c4a1b7e3d5c60`:
/// 13-digit unix ms, 4 lowercase hex counter, 16 lowercase hex node id.
#[derive(Debug, Clone)]
pub struct Hlc {
    node: String,
    last_ms: u64,
    counter: u32,
    fixed_wall_ms: Option<u64>,
}

impl Hlc {
    /// Creates a clock for `node` that reads the system wall clock.
    pub fn new(node: impl Into<String>) -> Self {
        let node = node.into();
        assert!(is_node_id(&node), "hlc node id must be 16 lowercase hex chars");
        Self { node, last_ms: 0, counter: 0, fixed_wall_ms: None }
    }

    /// Creates a clock with a fixed wall time in ms, for tests and replay.
    pub fn with_wall_clock(node: impl Into<String>, wall_ms: u64) -> Self {
        let mut clock = Self::new(node);
        clock.fixed_wall_ms = Some(wall_ms);
        clock
    }

    /// Overrides the wall time in ms the clock reads next.
    pub fn set_wall_clock(&mut self, wall_ms: u64) {
        self.fixed_wall_ms = Some(wall_ms);
    }

    fn wall_ms(&self) -> u64 {
        self.fixed_wall_ms.unwrap_or_else(|| {
            SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
        })
    }

    /// Advances the clock for a local event and returns its stamp.
    pub fn tick(&mut self) -> String {
        let previous = (self.last_ms, self.counter);
        let wall = self.wall_ms();
        if wall > self.last_ms {
            self.last_ms = wall;
            self.counter = 0;
        } else {
            self.bump_counter();
        }
        assert!((self.last_ms, self.counter) > previous, "hlc tick must issue a strictly greater stamp");
        self.stamp()
    }

    /// Merges a remote stamp and returns a local stamp ordered after it and after every prior local stamp.
    pub fn receive(&mut self, remote: &str) -> Result<String> {
        let (remote_ms, remote_counter) = parse(remote)?;
        let previous = (self.last_ms, self.counter);
        let wall = self.wall_ms();
        let next_ms = wall.max(self.last_ms).max(remote_ms);
        let next_counter = if next_ms == self.last_ms && next_ms == remote_ms {
            Some(self.counter.max(remote_counter))
        } else if next_ms == self.last_ms {
            Some(self.counter)
        } else if next_ms == remote_ms {
            Some(remote_counter)
        } else {
            None
        };
        self.last_ms = next_ms;
        match next_counter {
            Some(c) => {
                self.counter = c;
                self.bump_counter();
            }
            None => self.counter = 0,
        }
        assert!((self.last_ms, self.counter) > previous, "hlc receive must issue a strictly greater stamp");
        let stamp = self.stamp();
        assert!(stamp.as_str() > remote, "hlc receive must order after remote");
        Ok(stamp)
    }

    fn bump_counter(&mut self) {
        if self.counter >= COUNTER_MAX {
            self.last_ms += 1;
            self.counter = 0;
        } else {
            self.counter += 1;
        }
    }

    fn stamp(&self) -> String {
        assert!(self.last_ms < 10u64.pow(MS_DIGITS as u32), "hlc ms out of range");
        assert!(self.counter <= COUNTER_MAX, "hlc counter out of range");
        format!("{:0ms$}-{:0c$x}-{}", self.last_ms, self.counter, self.node, ms = MS_DIGITS, c = COUNTER_DIGITS)
    }
}

/// Parses a stamp into its ms and counter parts.
pub fn parse(stamp: &str) -> Result<(u64, u32)> {
    let bad = || Error::InvalidHlc(stamp.chars().take(64).collect());
    let mut parts = stamp.splitn(3, '-');
    let ms = parts.next().filter(|p| p.len() == MS_DIGITS).ok_or_else(bad)?;
    let counter = parts.next().filter(|p| p.len() == COUNTER_DIGITS).ok_or_else(bad)?;
    let node = parts.next().ok_or_else(bad)?;
    if !is_node_id(node) || !ms.bytes().all(|b| b.is_ascii_digit()) || !counter.bytes().all(is_lower_hex) {
        return Err(bad());
    }
    let counter = u32::from_str_radix(counter, 16).map_err(|_| bad())?;
    Ok((ms.parse().map_err(|_| bad())?, counter))
}

/// Returns a random 16 lowercase hex node id for this device; store it locally, never sync it.
pub fn random_node_id() -> String {
    let simple = uuid::Uuid::now_v7().simple().to_string();
    simple[simple.len() - NODE_DIGITS..].to_string()
}

fn is_node_id(node: &str) -> bool {
    node.len() == NODE_DIGITS && node.bytes().all(is_lower_hex)
}

fn is_lower_hex(b: u8) -> bool {
    b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
}

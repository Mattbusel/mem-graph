// SPDX-License-Identifier: MIT
//! Long-term memory for an LLM agent: store facts as a graph, recall them as
//! prompt-ready sentences, and keep them across runs in a JSON file.
//!
//! Run it with `cargo run --example agent_memory`.
//!
//! The agent framework is left out so the example runs without API keys. To
//! give a rig agent this memory, wrap `AgentMemory` in a struct that
//! implements `rig::tool::Tool` twice (or once with an `action` argument):
//!
//! - a `remember` tool whose arguments are `{ subject, relation, object }`
//!   and whose `call` runs [`AgentMemory::remember`] (or
//!   [`AgentMemory::remember_current`] for facts that replace an older one);
//! - a `recall` tool whose argument is `{ about }` and whose `call` returns
//!   [`AgentMemory::recall`] joined with newlines.
//!
//! Register both with `agent_builder.tool(...)`, keep the memory behind an
//! `Arc<Mutex<_>>` so both tools share it, and call `save` when the session
//! ends. Any other agent loop works the same way: the two calls are plain
//! functions from strings to strings.

use std::path::Path;

use chrono::{DateTime, Utc};
use mem_graph::{Direction, Entity, EntityId, GraphError, GraphStore, PropValue, Relationship};

/// Facts about the world, as `subject -relation-> object` triples.
struct AgentMemory {
    graph: GraphStore,
}

impl AgentMemory {
    /// Load the memory saved by a previous run, or start empty.
    fn open(path: &Path) -> Result<Self, GraphError> {
        let graph = if path.exists() { GraphStore::load_json(path)? } else { GraphStore::new() };
        Ok(Self { graph })
    }

    fn save(&self, path: &Path) -> Result<(), GraphError> {
        self.graph.save_json(path)
    }

    /// Entity ids are the lowercased names, so "Bob" and "bob" are one entity.
    fn ensure(&mut self, name: &str) -> EntityId {
        let id = EntityId::new(name.trim().to_lowercase());
        if self.graph.get_entity(&id).is_err() {
            let entity = Entity::new(id.clone(), "Thing").with_prop("name", PropValue::Text(name.trim().into()));
            self.graph.upsert_entity(entity);
        }
        id
    }

    /// Store a fact that can hold alongside others of the same relation
    /// ("alice knows bob" and "alice knows carol"). Storing it twice is a no-op.
    fn remember(&mut self, subject: &str, relation: &str, object: &str, now: DateTime<Utc>) -> Result<(), GraphError> {
        let (s, o) = (self.ensure(subject), self.ensure(object));
        if self.graph.get_relationship(&s, &o, relation).is_ok() {
            return Ok(());
        }
        self.graph.add_relationship(Relationship::new(s, o, relation).with_temporal(now, None))
    }

    /// Store a fact that replaces the current one of the same relation
    /// ("bob works_at acme" ends "bob works_at globex"). The old fact is kept
    /// with an end date, so the agent can still answer "where did bob work
    /// last year?".
    fn remember_current(&mut self, subject: &str, relation: &str, object: &str, now: DateTime<Utc>) -> Result<(), GraphError> {
        let s = self.ensure(subject);
        let o = EntityId::new(object.trim().to_lowercase());
        let ended: Vec<Relationship> = self
            .graph
            .relationships_of(&s, Direction::Outgoing)
            .into_iter()
            .filter(|r| r.rel_type == relation && r.to != o && r.is_valid_at(now))
            .cloned()
            .collect();
        for mut old in ended {
            self.graph.remove_relationship(&old.from, &old.to, &old.rel_type)?;
            old.valid_to = Some(now);
            self.graph.add_relationship(old)?;
        }
        self.remember(subject, relation, object, now)
    }

    fn name(&self, id: &EntityId) -> String {
        self.graph
            .get_entity(id)
            .ok()
            .and_then(|e| e.get_prop("name"))
            .and_then(|v| v.as_text().ok())
            .unwrap_or(id.as_str())
            .to_string()
    }

    /// Every fact that is true at `now` about any entity matching `about`
    /// (case-insensitive text match), plus the facts between its direct
    /// neighbors, as sentences ready to paste into a prompt.
    fn recall(&self, about: &str, now: DateTime<Utc>) -> Vec<String> {
        let mut facts = Vec::new();
        for hit in self.graph.search_entities(about) {
            let Ok(around) = self.graph.subgraph(&hit.id, 1, Direction::Both) else { continue };
            for rel in around.snapshot().relationships {
                if !rel.is_valid_at(now) {
                    continue;
                }
                let fact = format!("{} {} {}", self.name(&rel.from), rel.rel_type.replace('_', " "), self.name(&rel.to));
                if !facts.contains(&fact) {
                    facts.push(fact);
                }
            }
        }
        facts
    }
}

fn main() -> Result<(), GraphError> {
    let path = std::env::temp_dir().join(format!("mem-graph-agent-{}.json", std::process::id()));
    let start = Utc::now() - chrono::Duration::days(400);
    let later = Utc::now();

    // Session 1: the agent learns things from the conversation.
    {
        let mut memory = AgentMemory::open(&path)?;
        memory.remember_current("Bob", "works_at", "Globex", start)?;
        memory.remember("Bob", "knows", "Alice", start)?;
        memory.remember("Alice", "lives_in", "Berlin", start)?;
        memory.remember("Globex", "located_in", "Springfield", start)?;
        memory.save(&path)?;
        println!("session 1 saved {} entities and {} facts", memory.graph.entity_count(), memory.graph.edge_count());
    }

    // Session 2, a year later: the memory is loaded back from disk, a fact
    // changes, and the agent recalls what it knows before answering.
    {
        let mut memory = AgentMemory::open(&path)?;
        memory.remember_current("Bob", "works_at", "Acme", later)?;

        println!("\nprompt context for a question about Bob:");
        for fact in memory.recall("bob", later) {
            println!("  - {fact}");
        }

        let a_year_ago = later - chrono::Duration::days(365);
        println!("\nwhat was true about Bob a year ago:");
        for fact in memory.recall("bob", a_year_ago) {
            println!("  - {fact}");
        }

        // Structured questions do not need the text search.
        let bob = EntityId::new("bob");
        let employer: Vec<String> = memory
            .graph
            .neighbors_out_at(&bob, later)
            .into_iter()
            .filter(|(_, rel)| rel.rel_type == "works_at")
            .map(|(e, _)| memory.name(&e.id))
            .collect();
        println!("\nBob works at: {}", employer.join(", "));

        let link = memory.graph.shortest_path_where(&bob, &EntityId::new("berlin"), Direction::Both, |r| r.is_valid_at(later))?;
        if let Some(path) = link {
            let names: Vec<String> = path.iter().map(|id| memory.name(id)).collect();
            println!("how Bob is connected to Berlin: {}", names.join(" -> "));
        }

        assert_eq!(employer, ["Acme"]);
        memory.save(&path)?;
    }

    std::fs::remove_file(&path)?;
    Ok(())
}

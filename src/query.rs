// SPDX-License-Identifier: MIT
//! Query helpers: typed neighbor lookups, filtered shortest paths, subgraph
//! extraction and plain-text search over entities.
//!
//! Everything here is a method on [`GraphStore`]; this module only adds the
//! [`Direction`] type the methods take.
//!
//! ## Example
//! ```rust
//! use mem_graph::{Direction, Entity, EntityId, GraphStore, PropValue, Relationship};
//!
//! let id = EntityId::new;
//! let mut g = GraphStore::new();
//! g.add_entity(Entity::new(id("bob"), "Person").with_prop("name", PropValue::Text("Bob Ross".into()))).unwrap();
//! g.add_entity(Entity::new(id("acme"), "Company")).unwrap();
//! g.add_entity(Entity::new(id("berlin"), "City")).unwrap();
//! g.add_relationship(Relationship::new(id("bob"), id("acme"), "works_at")).unwrap();
//! g.add_relationship(Relationship::new(id("acme"), id("berlin"), "located_in")).unwrap();
//!
//! // Who does bob work for?
//! let employers = g.neighbors_out_by_type(&id("bob"), "works_at");
//! assert_eq!(employers[0].id.as_str(), "acme");
//!
//! // Find an entity by a fragment of its name, case-insensitively.
//! let hits = g.search_entities("ross");
//! assert_eq!(hits[0].id.as_str(), "bob");
//!
//! // Everything within one hop of acme, in either direction, as its own graph.
//! let around_acme = g.subgraph(&id("acme"), 1, Direction::Both).unwrap();
//! assert_eq!(around_acme.entity_count(), 3);
//! assert_eq!(around_acme.edge_count(), 2);
//! ```

use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::GraphError;
use crate::store::GraphStore;
use crate::types::{Entity, EntityId, PropValue, Relationship};

/// Which relationships a traversal follows from a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Direction {
    /// Follow relationships from `from` to `to` (the default).
    #[default]
    Outgoing,
    /// Follow relationships backwards, from `to` to `from`.
    Incoming,
    /// Follow relationships either way, treating the graph as undirected.
    Both,
}

impl GraphStore {
    fn require(&self, id: &EntityId) -> Result<(), GraphError> {
        if self.entities.contains_key(id.as_str()) {
            Ok(())
        } else {
            Err(GraphError::EntityNotFound(id.0.clone()))
        }
    }

    /// The store's own copy of `id`, borrowed for as long as the store.
    fn key_of(&self, id: &EntityId) -> Option<&str> {
        self.entities.get_key_value(id.as_str()).map(|(k, _)| k.as_str())
    }

    /// The nodes next to `node` in `direction`, with the relationship that
    /// links them: outgoing ones first (in insertion order), then incoming.
    fn adjacent<'a>(
        &'a self,
        node: &'a str,
        direction: Direction,
    ) -> impl Iterator<Item = (&'a str, &'a Relationship)> + 'a {
        let out = matches!(direction, Direction::Outgoing | Direction::Both)
            .then(|| self.out_edges.get(node))
            .flatten()
            .into_iter()
            .flatten()
            .filter_map(move |(to, rt)| self.edge(node, to, rt).map(|r| (to.as_str(), r)));
        let inc = matches!(direction, Direction::Incoming | Direction::Both)
            .then(|| self.in_edges.get(node))
            .flatten()
            .into_iter()
            .flatten()
            .filter_map(move |(from, rt)| self.edge(from, node, rt).map(|r| (from.as_str(), r)));
        out.chain(inc)
    }

    /// Entities reached from `id` through relationships of type `rel_type`,
    /// following them in `direction`. Each entity appears once, outgoing
    /// matches first (in insertion order), then incoming ones.
    ///
    /// Returns an empty list when `id` is not in the store.
    pub fn neighbors_by_type(&self, id: &EntityId, rel_type: &str, direction: Direction) -> Vec<&Entity> {
        let Some(node) = self.key_of(id) else { return Vec::new() };
        let mut seen: HashSet<&str> = HashSet::new();
        self.adjacent(node, direction)
            .filter(|(_, rel)| rel.rel_type == rel_type)
            .filter(|(other, _)| seen.insert(*other))
            .filter_map(|(other, _)| self.entities.get(other))
            .collect()
    }

    /// Targets of the outgoing `rel_type` relationships of `id`, in insertion
    /// order: `neighbors_out_by_type(&bob, "works_at")` is where bob works.
    ///
    /// Returns an empty list when `id` is not in the store.
    pub fn neighbors_out_by_type(&self, id: &EntityId, rel_type: &str) -> Vec<&Entity> {
        self.neighbors_by_type(id, rel_type, Direction::Outgoing)
    }

    /// Sources of the incoming `rel_type` relationships of `id`, in insertion
    /// order: `neighbors_in_by_type(&acme, "works_at")` is who works at acme.
    ///
    /// Returns an empty list when `id` is not in the store.
    pub fn neighbors_in_by_type(&self, id: &EntityId, rel_type: &str) -> Vec<&Entity> {
        self.neighbors_by_type(id, rel_type, Direction::Incoming)
    }

    /// The relationships that touch `id` in `direction` (outgoing first, in
    /// insertion order, then incoming). A self-loop is listed once per
    /// direction it matches.
    ///
    /// Returns an empty list when `id` is not in the store.
    pub fn relationships_of(&self, id: &EntityId, direction: Direction) -> Vec<&Relationship> {
        let Some(node) = self.key_of(id) else { return Vec::new() };
        self.adjacent(node, direction).map(|(_, rel)| rel).collect()
    }

    /// Every relationship of type `rel_type`, sorted by `(from, to)`.
    pub fn relationships_of_type(&self, rel_type: &str) -> Vec<&Relationship> {
        let mut rels: Vec<&Relationship> =
            self.edges.values().filter(|r| r.rel_type == rel_type).collect();
        rels.sort_unstable_by(|a, b| (&a.from.0, &a.to.0).cmp(&(&b.from.0, &b.to.0)));
        rels
    }

    /// Every entity whose `kind` is exactly `kind`, sorted by id.
    pub fn entities_of_kind(&self, kind: &str) -> Vec<&Entity> {
        let mut found: Vec<&Entity> = self.entities.values().filter(|e| e.kind == kind).collect();
        found.sort_unstable_by(|a, b| a.id.0.cmp(&b.id.0));
        found
    }

    /// Shortest path (fewest relationships) from `from` to `to` that only
    /// follows relationships for which `follow` returns `true`, moving in
    /// `direction`. With [`Direction::Both`] the graph is treated as
    /// undirected.
    ///
    /// This is the general form of [`shortest_path`](Self::shortest_path)
    /// (`Direction::Outgoing`, follow everything) and
    /// [`shortest_path_at`](Self::shortest_path_at) (follow what is valid at
    /// a time).
    ///
    /// # Returns
    /// - `Ok(Some(path))`: the entity ids from `from` to `to`, both included.
    ///   `from == to` gives `[from]`.
    /// - `Ok(None)`: no such path.
    ///
    /// # Errors
    /// [`GraphError::EntityNotFound`] if either endpoint is missing.
    pub fn shortest_path_where<F>(
        &self,
        from: &EntityId,
        to: &EntityId,
        direction: Direction,
        mut follow: F,
    ) -> Result<Option<Vec<EntityId>>, GraphError>
    where
        F: FnMut(&Relationship) -> bool,
    {
        self.require(from)?;
        self.require(to)?;
        if from == to {
            return Ok(Some(vec![from.clone()]));
        }
        let mut parent: HashMap<&str, &str> = HashMap::new();
        let mut visited: HashSet<&str> = HashSet::from([from.as_str()]);
        let mut queue: VecDeque<&str> = VecDeque::from([from.as_str()]);
        while let Some(current) = queue.pop_front() {
            for (next, rel) in self.adjacent(current, direction) {
                if visited.contains(next) || !follow(rel) {
                    continue;
                }
                visited.insert(next);
                parent.insert(next, current);
                if next == to.as_str() {
                    let mut path = vec![to.clone()];
                    let mut node = next;
                    while let Some(&p) = parent.get(node) {
                        path.push(EntityId::new(p));
                        node = p;
                    }
                    path.reverse();
                    return Ok(Some(path));
                }
                queue.push_back(next);
            }
        }
        Ok(None)
    }

    /// Shortest directed path from `from` to `to` that only uses
    /// relationships whose type is in `rel_types`, e.g. the reporting chain
    /// through `"reports_to"` while ignoring `"knows"`.
    ///
    /// # Errors
    /// [`GraphError::EntityNotFound`] if either endpoint is missing.
    pub fn shortest_path_via(
        &self,
        from: &EntityId,
        to: &EntityId,
        rel_types: &[&str],
    ) -> Result<Option<Vec<EntityId>>, GraphError> {
        self.shortest_path_where(from, to, Direction::Outgoing, |rel| {
            rel_types.contains(&rel.rel_type.as_str())
        })
    }

    /// Ids of every entity within `depth` hops of `start` moving in
    /// `direction`, in breadth-first order (`start` first).
    ///
    /// With `Direction::Outgoing` this is the same list as
    /// [`bfs`](Self::bfs).
    ///
    /// # Errors
    /// [`GraphError::EntityNotFound`] if `start` is not in the store.
    pub fn within(
        &self,
        start: &EntityId,
        depth: usize,
        direction: Direction,
    ) -> Result<Vec<EntityId>, GraphError> {
        self.require(start)?;
        let mut visited: HashSet<&str> = HashSet::from([start.as_str()]);
        let mut queue: VecDeque<(&str, usize)> = VecDeque::from([(start.as_str(), 0)]);
        let mut result = Vec::new();
        while let Some((current, d)) = queue.pop_front() {
            result.push(EntityId::new(current));
            if d >= depth {
                continue;
            }
            for (next, _) in self.adjacent(current, direction) {
                if visited.insert(next) {
                    queue.push_back((next, d + 1));
                }
            }
        }
        Ok(result)
    }

    /// Extract the neighborhood of `start` as a new, independent store: every
    /// entity within `depth` hops (moving in `direction`) and every
    /// relationship between two of those entities (the induced subgraph, as
    /// in NetworkX's `ego_graph`). `depth = 0` gives just `start`.
    ///
    /// Use it to hand an agent only the part of its memory that is relevant
    /// to the current question, or to save a slice of a large graph.
    ///
    /// # Errors
    /// [`GraphError::EntityNotFound`] if `start` is not in the store.
    pub fn subgraph(
        &self,
        start: &EntityId,
        depth: usize,
        direction: Direction,
    ) -> Result<GraphStore, GraphError> {
        let ids = self.within(start, depth, direction)?;
        self.induced_subgraph(&ids)
    }

    /// A new store with the given entities and every relationship whose both
    /// endpoints are among them. Repeated ids are ignored. Relationships are
    /// inserted in `(from, to, rel_type)` order.
    ///
    /// # Errors
    /// [`GraphError::EntityNotFound`] for the first id that is not in the
    /// store.
    pub fn induced_subgraph<'a, I>(&self, ids: I) -> Result<GraphStore, GraphError>
    where
        I: IntoIterator<Item = &'a EntityId>,
    {
        let mut sub = GraphStore::new();
        for id in ids {
            let entity = self.get_entity(id)?;
            if !sub.entities.contains_key(id.as_str()) {
                sub.add_entity(entity.clone())?;
            }
        }
        for rel in self.sorted_relationships() {
            if sub.entities.contains_key(rel.from.as_str()) && sub.entities.contains_key(rel.to.as_str()) {
                sub.add_relationship(rel.clone())?;
            }
        }
        Ok(sub)
    }

    /// Case-insensitive substring search over entities: an entity matches
    /// when `query` occurs in its id, its kind, or any of its text
    /// properties (including text inside list properties). Numbers,
    /// booleans and timestamps are not searched. Results are sorted by id.
    ///
    /// An empty `query` matches every entity.
    ///
    /// # Example
    /// ```rust
    /// use mem_graph::{Entity, EntityId, GraphStore, PropValue};
    ///
    /// let mut g = GraphStore::new();
    /// g.add_entity(Entity::new(EntityId::new("p1"), "Person")
    ///     .with_prop("name", PropValue::Text("Ada Lovelace".into()))).unwrap();
    /// g.add_entity(Entity::new(EntityId::new("p2"), "Person")
    ///     .with_prop("tags", PropValue::List(vec![PropValue::Text("lovelace fan".into())]))).unwrap();
    /// let ids: Vec<&str> = g.search_entities("LOVELACE").iter().map(|e| e.id.as_str()).collect();
    /// assert_eq!(ids, ["p1", "p2"]);
    /// ```
    pub fn search_entities(&self, query: &str) -> Vec<&Entity> {
        let needle = query.to_lowercase();
        self.sorted_matches(|e| {
            contains_ci(&e.id.0, &needle)
                || contains_ci(&e.kind, &needle)
                || e.properties.values().any(|v| text_matches(v, &needle))
        })
    }

    /// Case-insensitive substring search on one property: entities whose
    /// property `key` is text containing `query` (or a list holding such
    /// text). Results are sorted by id.
    pub fn search_entities_by_prop(&self, key: &str, query: &str) -> Vec<&Entity> {
        let needle = query.to_lowercase();
        self.sorted_matches(|e| e.properties.get(key).is_some_and(|v| text_matches(v, &needle)))
    }

    /// Entities whose property `key` equals `value` exactly (`PropValue`
    /// equality, so `Number(1.0)` does not equal `Text("1")`). Results are
    /// sorted by id.
    pub fn find_by_prop(&self, key: &str, value: &PropValue) -> Vec<&Entity> {
        self.sorted_matches(|e| e.properties.get(key) == Some(value))
    }

    fn sorted_matches<F: FnMut(&Entity) -> bool>(&self, mut keep: F) -> Vec<&Entity> {
        let mut found: Vec<&Entity> = self.entities.values().filter(|e| keep(e)).collect();
        found.sort_unstable_by(|a, b| a.id.0.cmp(&b.id.0));
        found
    }
}

/// `needle` must already be lowercase.
fn contains_ci(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(needle)
}

fn text_matches(value: &PropValue, needle: &str) -> bool {
    match value {
        PropValue::Text(s) => contains_ci(s, needle),
        PropValue::List(items) => items.iter().any(|v| text_matches(v, needle)),
        _ => false,
    }
}

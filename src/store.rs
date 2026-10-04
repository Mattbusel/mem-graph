// SPDX-License-Identifier: MIT
//! # Stage: GraphStore
//!
//! ## Responsibility
//! Maintain an in-memory directed graph of entities and typed relationships.
//! Provide O(1) entity lookup, O(degree) neighbor access, and BFS/DFS/shortest-path traversal.
//!
//! ## Guarantees
//! - Thread-safety: NOT guaranteed — wrap in `Arc<RwLock<>>` for concurrent access.
//! - Bounded: memory grows with entity and edge count.
//! - Non-panicking: all public methods return `Result`.
//!
//! ## NOT Responsible For
//! - Persistence (in-memory only)
//! - Distributed replication

use std::collections::{HashMap, HashSet, VecDeque};
use crate::error::GraphError;
use crate::types::{Entity, EntityId, Relationship};

/// The central in-memory graph store.
pub struct GraphStore {
    entities: HashMap<String, Entity>,
    /// Forward index: from_id → Vec<(to_id, rel_type)>
    out_edges: HashMap<String, Vec<(String, String)>>,
    /// Backward index: to_id → Vec<(from_id, rel_type)>
    in_edges: HashMap<String, Vec<(String, String)>>,
    /// Edge storage: (from, to, rel_type) → Relationship
    edges: HashMap<(String, String, String), Relationship>,
}

impl GraphStore {
    /// Create an empty graph store.
    ///
    /// # Example
    /// ```rust
    /// use mem_graph::GraphStore;
    /// let store = GraphStore::new();
    /// assert_eq!(store.entity_count(), 0);
    /// ```
    pub fn new() -> Self {
        Self {
            entities: HashMap::new(),
            out_edges: HashMap::new(),
            in_edges: HashMap::new(),
            edges: HashMap::new(),
        }
    }

    /// Insert an entity into the store.
    ///
    /// # Arguments
    /// * `entity` — The entity to insert.
    ///
    /// # Returns
    /// - `Ok(())` on success.
    /// - `Err(GraphError::DuplicateEntity)` if the id already exists.
    pub fn add_entity(&mut self, entity: Entity) -> Result<(), GraphError> {
        let id = entity.id.0.clone();
        if self.entities.contains_key(&id) {
            return Err(GraphError::DuplicateEntity(id));
        }
        self.entities.insert(id, entity);
        Ok(())
    }

    /// Get an entity by id — O(1).
    ///
    /// # Returns
    /// - `Ok(&Entity)` if found.
    /// - `Err(GraphError::EntityNotFound)` if not present.
    pub fn get_entity(&self, id: &EntityId) -> Result<&Entity, GraphError> {
        self.entities
            .get(id.as_str())
            .ok_or_else(|| GraphError::EntityNotFound(id.0.clone()))
    }

    /// Insert a directed relationship. Both endpoint entities must already exist.
    ///
    /// # Returns
    /// - `Ok(())` on success.
    /// - `Err(GraphError::EntityNotFound)` if either endpoint is missing.
    /// - `Err(GraphError::DuplicateRelationship)` if the exact triple exists.
    pub fn add_relationship(&mut self, rel: Relationship) -> Result<(), GraphError> {
        let from = rel.from.0.clone();
        let to = rel.to.0.clone();
        let rt = rel.rel_type.clone();

        if !self.entities.contains_key(&from) {
            return Err(GraphError::EntityNotFound(from));
        }
        if !self.entities.contains_key(&to) {
            return Err(GraphError::EntityNotFound(to));
        }

        let key = (from.clone(), to.clone(), rt.clone());
        if self.edges.contains_key(&key) {
            return Err(GraphError::DuplicateRelationship { from, to, rel: rt });
        }

        self.out_edges.entry(from.clone()).or_default().push((to.clone(), rt.clone()));
        self.in_edges.entry(to.clone()).or_default().push((from.clone(), rt.clone()));
        self.edges.insert(key, rel);
        Ok(())
    }

    /// Retrieve a specific relationship by its triple key.
    ///
    /// # Returns
    /// - `Ok(&Relationship)` if found.
    /// - `Err(GraphError::RelationshipNotFound)` otherwise.
    pub fn get_relationship(
        &self,
        from: &EntityId,
        to: &EntityId,
        rel_type: &str,
    ) -> Result<&Relationship, GraphError> {
        let key = (from.0.clone(), to.0.clone(), rel_type.to_string());
        self.edges.get(&key).ok_or_else(|| GraphError::RelationshipNotFound {
            from: from.0.clone(),
            to: to.0.clone(),
            rel: rel_type.to_string(),
        })
    }

    /// Get all outgoing neighbors of an entity — O(degree).
    ///
    /// Returns pairs of `(&Entity, rel_type)`.
    pub fn neighbors_out(&self, id: &EntityId) -> Vec<(&Entity, &str)> {
        self.out_edges
            .get(id.as_str())
            .map(|edges| {
                edges
                    .iter()
                    .filter_map(|(to, rt)| self.entities.get(to.as_str()).map(|e| (e, rt.as_str())))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all incoming neighbors of an entity — O(degree).
    ///
    /// Returns pairs of `(&Entity, rel_type)`.
    pub fn neighbors_in(&self, id: &EntityId) -> Vec<(&Entity, &str)> {
        self.in_edges
            .get(id.as_str())
            .map(|edges| {
                edges
                    .iter()
                    .filter_map(|(from, rt)| self.entities.get(from.as_str()).map(|e| (e, rt.as_str())))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// BFS traversal from a start node, up to `max_depth` hops.
    ///
    /// Returns visited entity ids in BFS order.
    ///
    /// # Returns
    /// - `Err(GraphError::EntityNotFound)` if `start` is not in the store.
    pub fn bfs(&self, start: &EntityId, max_depth: usize) -> Result<Vec<EntityId>, GraphError> {
        if !self.entities.contains_key(start.as_str()) {
            return Err(GraphError::EntityNotFound(start.0.clone()));
        }
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut result = Vec::new();

        queue.push_back((start.0.clone(), 0usize));
        visited.insert(start.0.clone());

        while let Some((current, depth)) = queue.pop_front() {
            result.push(EntityId::new(current.clone()));
            if depth >= max_depth { continue; }
            if let Some(edges) = self.out_edges.get(&current) {
                for (neighbor, _) in edges {
                    if !visited.contains(neighbor) {
                        visited.insert(neighbor.clone());
                        queue.push_back((neighbor.clone(), depth + 1));
                    }
                }
            }
        }
        Ok(result)
    }

    /// DFS traversal from a start node, up to `max_depth` hops.
    ///
    /// Returns visited entity ids in DFS order.
    ///
    /// # Returns
    /// - `Err(GraphError::EntityNotFound)` if `start` is not in the store.
    pub fn dfs(&self, start: &EntityId, max_depth: usize) -> Result<Vec<EntityId>, GraphError> {
        if !self.entities.contains_key(start.as_str()) {
            return Err(GraphError::EntityNotFound(start.0.clone()));
        }
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        self.dfs_inner(start.as_str(), 0, max_depth, &mut visited, &mut result);
        Ok(result)
    }

    fn dfs_inner(
        &self,
        current: &str,
        depth: usize,
        max_depth: usize,
        visited: &mut HashSet<String>,
        result: &mut Vec<EntityId>,
    ) {
        if visited.contains(current) { return; }
        visited.insert(current.to_string());
        result.push(EntityId::new(current));
        if depth >= max_depth { return; }
        if let Some(edges) = self.out_edges.get(current) {
            for (neighbor, _) in edges {
                self.dfs_inner(neighbor, depth + 1, max_depth, visited, result);
            }
        }
    }

    /// Find the shortest directed path from `from` to `to` using BFS.
    ///
    /// # Returns
    /// - `Ok(Some(path))` — ordered list of EntityIds from `from` to `to`.
    /// - `Ok(None)` — no path exists.
    /// - `Err(GraphError::EntityNotFound)` — if either endpoint is missing.
    pub fn shortest_path(
        &self,
        from: &EntityId,
        to: &EntityId,
    ) -> Result<Option<Vec<EntityId>>, GraphError> {
        if !self.entities.contains_key(from.as_str()) {
            return Err(GraphError::EntityNotFound(from.0.clone()));
        }
        if !self.entities.contains_key(to.as_str()) {
            return Err(GraphError::EntityNotFound(to.0.clone()));
        }
        if from == to {
            return Ok(Some(vec![from.clone()]));
        }

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut parent: HashMap<String, String> = HashMap::new();

        queue.push_back(from.0.clone());
        visited.insert(from.0.clone());

        while let Some(current) = queue.pop_front() {
            if let Some(edges) = self.out_edges.get(&current) {
                for (neighbor, _) in edges {
                    if !visited.contains(neighbor) {
                        visited.insert(neighbor.clone());
                        parent.insert(neighbor.clone(), current.clone());
                        if neighbor == &to.0 {
                            let mut path = vec![EntityId::new(neighbor.clone())];
                            let mut node = neighbor.clone();
                            while let Some(p) = parent.get(&node) {
                                path.push(EntityId::new(p.clone()));
                                node = p.clone();
                            }
                            path.reverse();
                            return Ok(Some(path));
                        }
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }
        Ok(None)
    }

    /// Compute the transitive closure: all entity ids reachable from `start`.
    ///
    /// # Returns
    /// - `Ok(HashSet<String>)` of reachable entity ids (including `start`).
    /// - `Err(GraphError::EntityNotFound)` if `start` is not in the store.
    pub fn transitive_closure(&self, start: &EntityId) -> Result<HashSet<String>, GraphError> {
        let visited = self.bfs(start, usize::MAX)?;
        Ok(visited.into_iter().map(|id| id.0).collect())
    }

    /// Return the number of entities in the store.
    pub fn entity_count(&self) -> usize { self.entities.len() }

    /// Return the number of edges in the store.
    pub fn edge_count(&self) -> usize { self.edges.len() }
}

impl Default for GraphStore {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Entity;

    fn e(id: &str) -> Entity { Entity::new(EntityId::new(id), "Node") }

    fn store_with_chain() -> GraphStore {
        let mut g = GraphStore::new();
        g.add_entity(e("a")).unwrap();
        g.add_entity(e("b")).unwrap();
        g.add_entity(e("c")).unwrap();
        g.add_relationship(Relationship::new(EntityId::new("a"), EntityId::new("b"), "links")).unwrap();
        g.add_relationship(Relationship::new(EntityId::new("b"), EntityId::new("c"), "links")).unwrap();
        g
    }

    #[test]
    fn test_store_add_entity_ok() {
        let mut g = GraphStore::new();
        assert!(g.add_entity(e("x")).is_ok());
        assert_eq!(g.entity_count(), 1);
    }

    #[test]
    fn test_store_duplicate_entity_returns_error() {
        let mut g = GraphStore::new();
        g.add_entity(e("x")).unwrap();
        let err = g.add_entity(e("x")).unwrap_err();
        assert!(matches!(err, GraphError::DuplicateEntity(_)));
    }

    #[test]
    fn test_store_get_entity_ok() {
        let mut g = GraphStore::new();
        g.add_entity(e("x")).unwrap();
        assert!(g.get_entity(&EntityId::new("x")).is_ok());
    }

    #[test]
    fn test_store_get_entity_not_found_returns_error() {
        let g = GraphStore::new();
        let err = g.get_entity(&EntityId::new("missing")).unwrap_err();
        assert!(matches!(err, GraphError::EntityNotFound(_)));
    }

    #[test]
    fn test_store_add_relationship_missing_from_entity_returns_error() {
        let mut g = GraphStore::new();
        g.add_entity(e("b")).unwrap();
        let err = g.add_relationship(
            Relationship::new(EntityId::new("z"), EntityId::new("b"), "r")
        ).unwrap_err();
        assert!(matches!(err, GraphError::EntityNotFound(_)));
    }

    #[test]
    fn test_store_add_relationship_missing_to_entity_returns_error() {
        let mut g = GraphStore::new();
        g.add_entity(e("a")).unwrap();
        let err = g.add_relationship(
            Relationship::new(EntityId::new("a"), EntityId::new("z"), "r")
        ).unwrap_err();
        assert!(matches!(err, GraphError::EntityNotFound(_)));
    }

    #[test]
    fn test_store_duplicate_relationship_returns_error() {
        let mut g = GraphStore::new();
        g.add_entity(e("a")).unwrap();
        g.add_entity(e("b")).unwrap();
        g.add_relationship(Relationship::new(EntityId::new("a"), EntityId::new("b"), "r")).unwrap();
        let err = g.add_relationship(
            Relationship::new(EntityId::new("a"), EntityId::new("b"), "r")
        ).unwrap_err();
        assert!(matches!(err, GraphError::DuplicateRelationship { .. }));
    }

    #[test]
    fn test_store_neighbors_out_correct_count() {
        let g = store_with_chain();
        let neighbors = g.neighbors_out(&EntityId::new("a"));
        assert_eq!(neighbors.len(), 1);
        assert_eq!(neighbors[0].0.id.0, "b");
    }

    #[test]
    fn test_store_neighbors_in_correct_count() {
        let g = store_with_chain();
        let neighbors = g.neighbors_in(&EntityId::new("b"));
        assert_eq!(neighbors.len(), 1);
        assert_eq!(neighbors[0].0.id.0, "a");
    }

    #[test]
    fn test_store_neighbors_empty_for_leaf() {
        let g = store_with_chain();
        let out = g.neighbors_out(&EntityId::new("c"));
        assert!(out.is_empty());
    }

    #[test]
    fn test_store_bfs_visits_all_nodes() {
        let g = store_with_chain();
        let visited = g.bfs(&EntityId::new("a"), 10).unwrap();
        assert_eq!(visited.len(), 3);
    }

    #[test]
    fn test_store_bfs_respects_depth_limit() {
        let g = store_with_chain();
        let visited = g.bfs(&EntityId::new("a"), 1).unwrap();
        // depth=0: a, depth=1: b (stops before c)
        assert_eq!(visited.len(), 2);
    }

    #[test]
    fn test_store_bfs_unknown_start_returns_error() {
        let g = GraphStore::new();
        let err = g.bfs(&EntityId::new("x"), 5).unwrap_err();
        assert!(matches!(err, GraphError::EntityNotFound(_)));
    }

    #[test]
    fn test_store_dfs_visits_all_nodes() {
        let g = store_with_chain();
        let visited = g.dfs(&EntityId::new("a"), 10).unwrap();
        assert_eq!(visited.len(), 3);
    }

    #[test]
    fn test_store_dfs_unknown_start_returns_error() {
        let g = GraphStore::new();
        let err = g.dfs(&EntityId::new("x"), 5).unwrap_err();
        assert!(matches!(err, GraphError::EntityNotFound(_)));
    }

    #[test]
    fn test_store_shortest_path_direct() {
        let g = store_with_chain();
        let path = g.shortest_path(&EntityId::new("a"), &EntityId::new("b")).unwrap();
        assert_eq!(path.unwrap().len(), 2);
    }

    #[test]
    fn test_store_shortest_path_two_hops() {
        let g = store_with_chain();
        let path = g.shortest_path(&EntityId::new("a"), &EntityId::new("c")).unwrap();
        assert_eq!(path.unwrap().len(), 3);
    }

    #[test]
    fn test_store_shortest_path_self_is_trivial() {
        let mut g = GraphStore::new();
        g.add_entity(e("x")).unwrap();
        let path = g.shortest_path(&EntityId::new("x"), &EntityId::new("x")).unwrap();
        assert_eq!(path.unwrap().len(), 1);
    }

    #[test]
    fn test_store_shortest_path_no_path_returns_none() {
        let mut g = GraphStore::new();
        g.add_entity(e("x")).unwrap();
        g.add_entity(e("y")).unwrap();
        let path = g.shortest_path(&EntityId::new("x"), &EntityId::new("y")).unwrap();
        assert!(path.is_none());
    }

    #[test]
    fn test_store_shortest_path_missing_from_returns_error() {
        let mut g = GraphStore::new();
        g.add_entity(e("y")).unwrap();
        let err = g.shortest_path(&EntityId::new("x"), &EntityId::new("y")).unwrap_err();
        assert!(matches!(err, GraphError::EntityNotFound(_)));
    }

    #[test]
    fn test_store_shortest_path_missing_to_returns_error() {
        let mut g = GraphStore::new();
        g.add_entity(e("x")).unwrap();
        let err = g.shortest_path(&EntityId::new("x"), &EntityId::new("y")).unwrap_err();
        assert!(matches!(err, GraphError::EntityNotFound(_)));
    }

    #[test]
    fn test_store_transitive_closure_includes_all_reachable() {
        let g = store_with_chain();
        let closure = g.transitive_closure(&EntityId::new("a")).unwrap();
        assert!(closure.contains("a"));
        assert!(closure.contains("b"));
        assert!(closure.contains("c"));
    }

    #[test]
    fn test_store_transitive_closure_unknown_start_returns_error() {
        let g = GraphStore::new();
        let err = g.transitive_closure(&EntityId::new("x")).unwrap_err();
        assert!(matches!(err, GraphError::EntityNotFound(_)));
    }

    #[test]
    fn test_store_get_relationship_ok() {
        let g = store_with_chain();
        let rel = g.get_relationship(&EntityId::new("a"), &EntityId::new("b"), "links");
        assert!(rel.is_ok());
    }

    #[test]
    fn test_store_get_relationship_not_found_returns_error() {
        let g = store_with_chain();
        let err = g.get_relationship(
            &EntityId::new("a"), &EntityId::new("c"), "direct"
        ).unwrap_err();
        assert!(matches!(err, GraphError::RelationshipNotFound { .. }));
    }

    #[test]
    fn test_store_edge_count_correct() {
        let g = store_with_chain();
        assert_eq!(g.edge_count(), 2);
    }

    #[test]
    fn test_store_default_is_empty() {
        let g = GraphStore::default();
        assert_eq!(g.entity_count(), 0);
        assert_eq!(g.edge_count(), 0);
    }
}

// ── Updates, snapshots and point-in-time queries ─────────────────────────────

impl GraphStore {
    /// Remove an entity and every relationship that starts or ends at it.
    ///
    /// # Errors
    /// [`GraphError::EntityNotFound`] if there is no such entity.
    pub fn remove_entity(&mut self, id: &EntityId) -> Result<Entity, GraphError> {
        let entity = self
            .entities
            .remove(id.as_str())
            .ok_or_else(|| GraphError::EntityNotFound(id.0.clone()))?;
        for (to, rt) in self.out_edges.remove(id.as_str()).unwrap_or_default() {
            self.edges.remove(&(id.0.clone(), to.clone(), rt.clone()));
            if let Some(list) = self.in_edges.get_mut(&to) {
                list.retain(|(f, r)| !(f == id.as_str() && r == &rt));
            }
        }
        for (from, rt) in self.in_edges.remove(id.as_str()).unwrap_or_default() {
            self.edges.remove(&(from.clone(), id.0.clone(), rt.clone()));
            if let Some(list) = self.out_edges.get_mut(&from) {
                list.retain(|(t, r)| !(t == id.as_str() && r == &rt));
            }
        }
        Ok(entity)
    }

    /// Remove one relationship.
    ///
    /// # Errors
    /// [`GraphError::RelationshipNotFound`] if there is no such relationship.
    pub fn remove_relationship(
        &mut self,
        from: &EntityId,
        to: &EntityId,
        rel_type: &str,
    ) -> Result<Relationship, GraphError> {
        let key = (from.0.clone(), to.0.clone(), rel_type.to_string());
        let rel = self.edges.remove(&key).ok_or_else(|| GraphError::RelationshipNotFound {
            from: from.0.clone(),
            to: to.0.clone(),
            rel: rel_type.to_string(),
        })?;
        if let Some(list) = self.out_edges.get_mut(from.as_str()) {
            list.retain(|(t, r)| !(t == to.as_str() && r == rel_type));
        }
        if let Some(list) = self.in_edges.get_mut(to.as_str()) {
            list.retain(|(f, r)| !(f == from.as_str() && r == rel_type));
        }
        Ok(rel)
    }

    /// Insert an entity, or replace the one with the same id (its
    /// relationships are kept). Returns the previous version.
    pub fn upsert_entity(&mut self, entity: Entity) -> Option<Entity> {
        self.entities.insert(entity.id.0.clone(), entity)
    }

    /// All entities, in no particular order.
    pub fn entities(&self) -> impl Iterator<Item = &Entity> {
        self.entities.values()
    }

    /// All relationships, in no particular order.
    pub fn relationships(&self) -> impl Iterator<Item = &Relationship> {
        self.edges.values()
    }

    /// Copy the whole graph into a [`GraphSnapshot`](crate::serial::GraphSnapshot)
    /// (for `to_json` and `restore_into`).
    pub fn snapshot(&self) -> crate::serial::GraphSnapshot {
        crate::serial::GraphSnapshot {
            entities: self.entities.values().cloned().collect(),
            relationships: self.edges.values().cloned().collect(),
        }
    }

    fn edge(&self, from: &str, to: &str, rel_type: &str) -> Option<&Relationship> {
        self.edges.get(&(from.to_string(), to.to_string(), rel_type.to_string()))
    }

    /// Outgoing neighbors through relationships valid at time `t`
    /// (see [`Relationship::is_valid_at`]).
    pub fn neighbors_out_at(
        &self,
        id: &EntityId,
        t: chrono::DateTime<chrono::Utc>,
    ) -> Vec<(&Entity, &Relationship)> {
        self.out_edges
            .get(id.as_str())
            .into_iter()
            .flatten()
            .filter_map(|(to, rt)| {
                let rel = self.edge(id.as_str(), to, rt)?;
                let entity = self.entities.get(to.as_str())?;
                rel.is_valid_at(t).then_some((entity, rel))
            })
            .collect()
    }

    /// [`bfs`](Self::bfs) that only follows relationships valid at time `t`:
    /// the graph as it was (or will be) at that moment.
    ///
    /// # Errors
    /// [`GraphError::EntityNotFound`] if `start` is not in the store.
    pub fn bfs_at(
        &self,
        start: &EntityId,
        max_depth: usize,
        t: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<EntityId>, GraphError> {
        if !self.entities.contains_key(start.as_str()) {
            return Err(GraphError::EntityNotFound(start.0.clone()));
        }
        let mut visited: HashSet<String> = HashSet::from([start.0.clone()]);
        let mut queue = VecDeque::from([(start.0.clone(), 0usize)]);
        let mut result = Vec::new();
        while let Some((current, depth)) = queue.pop_front() {
            result.push(EntityId::new(current.clone()));
            if depth >= max_depth {
                continue;
            }
            for (next, rt) in self.out_edges.get(&current).into_iter().flatten() {
                let valid = self.edge(&current, next, rt).is_some_and(|r| r.is_valid_at(t));
                if valid && visited.insert(next.clone()) {
                    queue.push_back((next.clone(), depth + 1));
                }
            }
        }
        Ok(result)
    }

    /// [`shortest_path`](Self::shortest_path) over relationships valid at time `t`.
    ///
    /// # Errors
    /// [`GraphError::EntityNotFound`] if either endpoint is missing.
    pub fn shortest_path_at(
        &self,
        from: &EntityId,
        to: &EntityId,
        t: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<Vec<EntityId>>, GraphError> {
        for id in [from, to] {
            if !self.entities.contains_key(id.as_str()) {
                return Err(GraphError::EntityNotFound(id.0.clone()));
            }
        }
        if from == to {
            return Ok(Some(vec![from.clone()]));
        }
        let mut parent: HashMap<String, String> = HashMap::new();
        let mut visited: HashSet<String> = HashSet::from([from.0.clone()]);
        let mut queue = VecDeque::from([from.0.clone()]);
        while let Some(current) = queue.pop_front() {
            for (next, rt) in self.out_edges.get(&current).into_iter().flatten() {
                let valid = self.edge(&current, next, rt).is_some_and(|r| r.is_valid_at(t));
                if !valid || !visited.insert(next.clone()) {
                    continue;
                }
                parent.insert(next.clone(), current.clone());
                if next == &to.0 {
                    let mut path = vec![to.clone()];
                    let mut node = next.clone();
                    while let Some(p) = parent.get(&node) {
                        path.push(EntityId::new(p.clone()));
                        node = p.clone();
                    }
                    path.reverse();
                    return Ok(Some(path));
                }
                queue.push_back(next.clone());
            }
        }
        Ok(None)
    }
}

// ── petgraph interop ─────────────────────────────────────────────────────────

/// Node index of every entity, returned by [`GraphStore::to_petgraph`].
#[cfg(feature = "petgraph")]
pub type NodeMap = HashMap<EntityId, petgraph::stable_graph::NodeIndex>;

#[cfg(feature = "petgraph")]
impl GraphStore {
    /// Copy the graph into a [`petgraph`] `StableDiGraph`, to run any petgraph
    /// algorithm on it (strongly connected components, topological sort,
    /// dominators, PageRank, ...). Returns the graph and the node index of
    /// every entity.
    pub fn to_petgraph(&self) -> (petgraph::stable_graph::StableDiGraph<Entity, Relationship>, NodeMap) {
        let mut graph =
            petgraph::stable_graph::StableDiGraph::with_capacity(self.entities.len(), self.edges.len());
        let mut nodes = NodeMap::with_capacity(self.entities.len());
        for entity in self.entities.values() {
            nodes.insert(entity.id.clone(), graph.add_node(entity.clone()));
        }
        for rel in self.edges.values() {
            if let (Some(&a), Some(&b)) = (nodes.get(&rel.from), nodes.get(&rel.to)) {
                graph.add_edge(a, b, rel.clone());
            }
        }
        (graph, nodes)
    }

    /// Build a store from a petgraph graph whose nodes are [`Entity`] and
    /// edges are [`Relationship`] (each edge's endpoints come from the graph).
    ///
    /// # Errors
    /// Duplicate entity ids or relationships.
    pub fn from_petgraph(
        graph: &petgraph::stable_graph::StableDiGraph<Entity, Relationship>,
    ) -> Result<Self, GraphError> {
        use petgraph::visit::{EdgeRef, IntoEdgeReferences};
        let mut store = Self::new();
        for idx in graph.node_indices() {
            store.add_entity(graph[idx].clone())?;
        }
        for edge in graph.edge_references() {
            let mut rel = edge.weight().clone();
            rel.from = graph[edge.source()].id.clone();
            rel.to = graph[edge.target()].id.clone();
            store.add_relationship(rel)?;
        }
        Ok(store)
    }

    /// Cheapest directed path where each relationship costs its numeric
    /// property `weight_prop` (1.0 when the property is missing), using
    /// petgraph's Dijkstra-style A* search. Returns the total cost and the
    /// path.
    ///
    /// # Errors
    /// [`GraphError::EntityNotFound`] for a missing endpoint, and
    /// [`GraphError::InvalidProperty`] when a weight is not a non-negative number.
    pub fn shortest_path_weighted(
        &self,
        from: &EntityId,
        to: &EntityId,
        weight_prop: &str,
    ) -> Result<Option<(f64, Vec<EntityId>)>, GraphError> {
        for rel in self.edges.values() {
            if let Some(v) = rel.properties.get(weight_prop) {
                let w = v.as_number().unwrap_or(f64::NAN);
                if !(w.is_finite() && w >= 0.0) {
                    return Err(GraphError::InvalidProperty(format!(
                        "'{weight_prop}' on {} -[{}]-> {} must be a non-negative number",
                        rel.from.0, rel.rel_type, rel.to.0
                    )));
                }
            }
        }
        let (graph, nodes) = self.to_petgraph();
        let start = *nodes.get(from).ok_or_else(|| GraphError::EntityNotFound(from.0.clone()))?;
        let goal = *nodes.get(to).ok_or_else(|| GraphError::EntityNotFound(to.0.clone()))?;
        let found = petgraph::algo::astar(
            &graph,
            start,
            |n| n == goal,
            |e| {
                e.weight().properties.get(weight_prop).and_then(|v| v.as_number().ok()).unwrap_or(1.0)
            },
            |_| 0.0,
        );
        Ok(found.map(|(total, path)| (total, path.into_iter().map(|n| graph[n].id.clone()).collect())))
    }
}

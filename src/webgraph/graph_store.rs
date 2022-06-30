// Cuely is an open source web search engine.
// Copyright (C) 2022 Cuely ApS
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use std::{
    cell::RefCell, collections::HashMap, hash::Hash, marker::PhantomData, ops::Div, path::Path,
};

use lru::LruCache;
use serde::{de::DeserializeOwned, Serialize};

use super::{kv::Kv, Edge, EdgeIterator, Node, NodeID, Store, StoredEdge};
pub(crate) struct Adjacency {
    pub(crate) tree: BlockedCachedTree<NodeID, Vec<StoredEdge>>,
}

impl Adjacency {
    fn insert(&mut self, from: NodeID, to: NodeID, label: String) {
        self.tree.insert(from, &mut |block| {
            block.entry(from).or_default().push(StoredEdge {
                other: to,
                label: label.clone(),
            })
        });
    }

    fn edges(&mut self, node: NodeID) -> Vec<StoredEdge> {
        self.tree.get(&node).cloned().unwrap_or_default()
    }
}

pub(crate) struct BlockedCachedTree<K, V>
where
    K: Hash + Eq + Serialize + Clone + Div<u64, Output = u64> + DeserializeOwned,
    V: Serialize + DeserializeOwned + Clone,
{
    pub(crate) inner: CachedTree<u64, HashMap<K, V>>,
    pub(crate) block_size: u64,
}

impl<K, V> BlockedCachedTree<K, V>
where
    K: Hash + Eq + Serialize + Clone + Div<u64, Output = u64> + DeserializeOwned,
    V: Serialize + DeserializeOwned + Clone,
{
    fn insert<B>(&mut self, key: K, mutate_block: &mut B)
    where
        B: FnMut(&mut HashMap<K, V>),
    {
        let block_id = key / self.block_size;

        if let Some(block) = self.inner.get_mut(&block_id) {
            // block.entry(key).or_default().push(value);
            // block.insert(key, value);
            mutate_block(block);
            return;
        }

        let mut new_block = HashMap::new();
        mutate_block(&mut new_block);

        self.inner.insert(block_id, new_block);
    }

    fn get(&mut self, key: &K) -> Option<&V> {
        let block_id = key.clone() / self.block_size;
        self.inner.get(&block_id).and_then(|block| block.get(key))
    }
}

pub(crate) struct CachedTree<K, V>
where
    K: Hash + Eq + Serialize + DeserializeOwned + Clone,
    V: Serialize + DeserializeOwned + Clone,
{
    pub(crate) store: Box<dyn Kv<K, V> + Send + Sync>,
    pub(crate) cache: LruCache<K, V>,
}

impl<K, V> CachedTree<K, V>
where
    K: Hash + Eq + Serialize + DeserializeOwned + Clone,
    V: Serialize + DeserializeOwned + Clone,
{
    pub(crate) fn new(store: Box<dyn Kv<K, V> + Send + Sync>, cache_size: usize) -> Self {
        Self {
            store,
            cache: LruCache::new(cache_size),
        }
    }

    fn update_cache(&mut self, key: &K) {
        if !self.cache.contains(key) {
            let val = self.store.get(key);

            if let Some(val) = val {
                self.cache.put(key.clone(), val);
            }
        }
    }

    pub(crate) fn get(&mut self, key: &K) -> Option<&V> {
        self.update_cache(key);
        self.cache.get(key)
    }

    fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.update_cache(key);
        self.cache.get_mut(key)
    }

    fn insert(&mut self, key: K, value: V) {
        if self.cache.len() == self.cache.cap() {
            if let Some((key, value)) = self.cache.pop_lru() {
                self.store.insert(key, value);
            }
        }

        self.cache.push(key, value);
    }

    fn flush(&mut self) {
        for (key, value) in self.cache.iter() {
            self.store.insert(key.clone(), value.clone());
        }

        self.store.flush();
    }

    fn iter(&self) -> impl Iterator<Item = (K, V)> + '_ {
        self.store.iter()
    }
}

pub struct GraphStore<S> {
    pub(crate) adjacency: RefCell<Adjacency>,
    pub(crate) reversed_adjacency: RefCell<Adjacency>,
    pub(crate) node2id: RefCell<CachedTree<Node, NodeID>>,
    pub(crate) id2node: RefCell<BlockedCachedTree<NodeID, Node>>,
    pub(crate) meta: RefCell<CachedTree<String, u64>>,
    pub(crate) store: PhantomData<S>,
}

impl<S: Store> GraphStore<S> {
    #[cfg(test)]
    pub(crate) fn temporary() -> GraphStore<S> {
        S::temporary()
    }

    pub fn open<P: AsRef<Path>>(path: P) -> Self {
        S::open(path)
    }

    fn next_id(&self) -> NodeID {
        self.meta
            .borrow_mut()
            .get(&"next_id".to_string())
            .cloned()
            .unwrap_or(0)
    }

    fn increment_next_id(&self) {
        let current_id = self.next_id();
        let next_id = current_id + 1;
        self.meta
            .borrow_mut()
            .insert("next_id".to_string(), next_id);
    }

    fn id_and_increment(&self) -> NodeID {
        let id = self.next_id();
        self.increment_next_id();
        id
    }

    fn assign_id(&self, node: Node, id: NodeID) {
        self.node2id.borrow_mut().insert(node.clone(), id);
        self.id2node.borrow_mut().insert(id, &mut |block| {
            block.insert(id, node.clone());
        });
    }

    fn id_or_assign(&self, node: Node) -> NodeID {
        if let Some(id) = self.node2id.borrow_mut().get(&node) {
            return *id;
        }
        let id = self.id_and_increment();
        self.assign_id(node, id);
        id
    }
    pub fn outgoing_edges(&self, node: NodeID) -> Vec<Edge> {
        self.adjacency
            .borrow_mut()
            .edges(node)
            .into_iter()
            .map(|edge| Edge {
                from: node,
                to: edge.other,
                label: edge.label,
            })
            .collect()
    }

    pub fn ingoing_edges(&self, node: NodeID) -> Vec<Edge> {
        self.reversed_adjacency
            .borrow_mut()
            .edges(node)
            .into_iter()
            .map(|edge| Edge {
                from: edge.other,
                to: node,
                label: edge.label,
            })
            .collect()
    }

    pub fn nodes(&self) -> impl Iterator<Item = NodeID> {
        self.node2id
            .borrow_mut()
            .iter()
            .map(|(_, id)| id)
            .collect::<Vec<u64>>()
            .into_iter()
    }

    pub fn insert(&mut self, from: Node, to: Node, label: String) {
        let from_id = self.id_or_assign(from);
        let to_id = self.id_or_assign(to);

        self.adjacency
            .borrow_mut()
            .insert(from_id, to_id, label.clone());
        self.reversed_adjacency
            .borrow_mut()
            .insert(to_id, from_id, label);
    }

    pub fn node2id(&self, node: &Node) -> Option<NodeID> {
        self.node2id.borrow_mut().get(node).cloned()
    }

    pub fn id2node(&self, id: &NodeID) -> Option<Node> {
        self.id2node.borrow_mut().get(id).cloned()
    }

    pub fn flush(&self) {
        self.adjacency.borrow_mut().tree.inner.flush();
        self.reversed_adjacency.borrow_mut().tree.inner.flush();
        self.node2id.borrow_mut().flush();
        self.id2node.borrow_mut().inner.flush();
        self.meta.borrow_mut().flush();
    }

    pub fn edges(&self) -> EdgeIterator<'_> {
        self.flush();

        EdgeIterator::new(&self.adjacency)
    }

    pub fn append(&mut self, other: GraphStore<S>) {
        for edge in other.edges() {
            let from = other.id2node(&edge.from).expect("node not found");
            let to = other.id2node(&edge.to).expect("node not found");

            self.insert(from, to, edge.label);
        }
    }
}

#[cfg(test)]
mod test {
    use crate::webgraph::rocksdb_store::RocksDbStore;

    use super::*;

    #[test]
    fn simple_triangle_graph() {
        //     ┌────┐
        //     │    │
        // ┌───A◄─┐ │
        // │      │ │
        // ▼      │ │
        // B─────►C◄┘

        let mut store: GraphStore<RocksDbStore> = GraphStore::temporary();

        let a = Node {
            name: "A".to_string(),
        };
        let b = Node {
            name: "B".to_string(),
        };
        let c = Node {
            name: "C".to_string(),
        };

        store.insert(a.clone(), b.clone(), String::new());
        store.insert(b.clone(), c.clone(), String::new());
        store.insert(c.clone(), a.clone(), String::new());
        store.insert(a.clone(), c.clone(), String::new());

        store.flush();

        let nodes: Vec<Node> = store
            .nodes()
            .map(|id| store.id2node.borrow_mut().get(&id).unwrap().clone())
            .collect();
        assert_eq!(nodes, vec![a.clone(), b.clone(), c.clone()]);

        let a_id = store.node2id(&a).unwrap();
        let b_id = store.node2id(&b).unwrap();
        let c_id = store.node2id(&c).unwrap();

        assert_eq!(
            store.outgoing_edges(a_id),
            vec![
                Edge {
                    from: a_id,
                    to: b_id,
                    label: String::new()
                },
                Edge {
                    from: a_id,
                    to: c_id,
                    label: String::new()
                },
            ]
        );

        assert_eq!(
            store.outgoing_edges(b_id),
            vec![Edge {
                from: b_id,
                to: c_id,
                label: String::new()
            },]
        );

        assert_eq!(
            store.ingoing_edges(c_id),
            vec![
                Edge {
                    from: b_id,
                    to: c_id,
                    label: String::new()
                },
                Edge {
                    from: a_id,
                    to: c_id,
                    label: String::new()
                },
            ]
        );

        assert_eq!(
            store.ingoing_edges(a_id),
            vec![Edge {
                from: c_id,
                to: a_id,
                label: String::new()
            },]
        );

        assert_eq!(
            store.ingoing_edges(b_id),
            vec![Edge {
                from: a_id,
                to: b_id,
                label: String::new()
            },]
        );
    }
}

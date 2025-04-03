use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

use tokio::sync::RwLock;

#[derive(Debug)]
pub struct Node<T> {
    id: usize,
    data: T,
    parents: HashMap<usize, Weak<RwLock<Node<T>>>>,
    children: HashMap<usize, Arc<RwLock<Node<T>>>>,
}

impl<T> Node<T> {
    pub fn new(id: usize, data: T) -> Self {
        Self {
            id,
            data,
            parents: HashMap::new(),
            children: HashMap::new(),
        }
    }

    pub fn build(id: usize, data: T) -> Arc<RwLock<Node<T>>> {
        Arc::new(RwLock::new(Node::new(id, data)))
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn data(&self) -> &T {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut T {
        &mut self.data
    }

    pub fn set_data(&mut self, value: T) {
        self.data = value
    }

    pub async fn add_child(&mut self, child: Arc<RwLock<Node<T>>>) {
        let id = child.read().await.id;
        if !self.children.contains_key(&id) {
            self.children.insert(id, Arc::clone(&child));
        }
    }

    pub fn remove_child(&mut self, id: usize) {
        self.children.remove(&id);
    }

    pub fn has_child(&self, id: usize) -> bool {
        self.children.contains_key(&id)
    }

    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    pub fn get_child(&self, id: usize) -> Option<Arc<RwLock<Node<T>>>> {
        self.children.get(&id).map(|child| child.clone())
    }

    pub async fn add_parent(&mut self, parent: Arc<RwLock<Node<T>>>) {
        let id = parent.read().await.id;
        if !self.parents.contains_key(&id) {
            self.parents.insert(id, Arc::downgrade(&parent));
        }
    }

    pub fn remove_parent(&mut self, id: usize) {
        self.parents.remove(&id);
    }

    pub fn has_parent(&self, id: usize) -> bool {
        match self.parents.get(&id) {
            Some(parent_ref) => parent_ref.upgrade().is_some(),
            None => false,
        }
    }

    pub fn has_parents(&self) -> bool {
        !(self.parents.is_empty())
            && self
                .parents
                .values()
                .any(|parent| parent.upgrade().is_some())
    }

    pub fn get_parent(&self, id: usize) -> Option<Arc<RwLock<Node<T>>>> {
        self.parents
            .get(&id)
            .and_then(|parent_ref| parent_ref.upgrade())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_new_node() {
        let node = Node::new(0, "test");
        let vec_node = Node::new(1, vec![1, 2, 3]);

        assert_eq!(node.id, 0);
        assert_eq!(node.data, "test");
        assert!(node.parents.is_empty());
        assert!(node.children.is_empty());

        assert_eq!(vec_node.id(), 1);
        assert_eq!(vec_node.data().len(), 3);
    }

    #[test]
    fn test_id_accessor_should_return_correct_id_value() {
        let node = Node::new(0, "test");

        assert_eq!(node.id(), 0);
    }

    #[test]
    fn test_data_accessor_should_return_correct_data_ref() {
        let node = Node::new(0, "test");

        assert_eq!(node.data, "test");
    }

    #[test]
    fn test_data_mut_accessor_allowed_to_change_node_data() {
        let mut node = Node::new(0, "test");

        let data_mut = node.data_mut();
        *data_mut = "new test";

        assert_eq!(node.data(), &"new test");
    }

    #[test]
    fn test_set_data_should_change_node_data() {
        let mut node = Node::new(0, "test");

        node.set_data("new test");

        assert_eq!(node.data(), &"new test");
    }

    #[tokio::test]
    async fn test_linking_two_nodes() {
        let root_node = Node::build(0, "root");
        let child_node = Node::build(1, "child");

        root_node.write().await.add_child(child_node.clone()).await;

        assert!(!root_node.read().await.has_parents());
        assert!(root_node.read().await.has_children());
        assert!(!child_node.read().await.has_parents());
        assert!(!child_node.read().await.has_children());

        child_node.write().await.add_parent(root_node.clone()).await;

        assert!(!root_node.read().await.has_parents());
        assert!(root_node.read().await.has_children());
        assert!(child_node.read().await.has_parents());
        assert!(!child_node.read().await.has_children());
    }

    #[tokio::test]
    async fn test_has_child_should_return_true_with_correct_child_id() {
        let node0 = Node::build(0, "root");
        let node1 = Node::build(1, "child");

        node0.write().await.add_child(node1.clone()).await;

        assert!(node0.read().await.has_child(1));
    }

    #[test]
    fn test_has_child_should_return_false_with_incorrect_child_id() {
        let node = Node::new(0, "test");

        assert!(!node.has_child(1));
    }

    #[tokio::test]
    async fn test_has_parent_should_return_true_with_correct_parent_id() {
        let node0 = Node::build(0, "n0");
        let node1 = Node::build(1, "n1");

        node1.write().await.add_parent(node0.clone()).await;

        assert!(node1.read().await.has_parent(0));
    }

    #[test]
    fn test_has_parent_should_return_false_with_incorrect_parent_id() {
        let node = Node::new(0, "test");

        assert!(!node.has_parent(1));
    }

    #[tokio::test]
    async fn test_has_parent_should_return_false_after_parent_dropped() {
        let child = Node::build(1, "child");

        {
            let root = Node::build(0, "root");

            child.write().await.add_parent(root.clone()).await;

            assert!(child.read().await.has_parent(0));
            // root dropped here
        }

        assert!(child.read().await.get_parent(0).is_none());
        assert!(!child.read().await.has_parents());
        assert!(!child.read().await.has_parent(0));
    }
}

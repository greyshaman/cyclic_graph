use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::spawn;
use tokio::sync::RwLock;

use crate::Error;
use crate::weak_node_wrapper::WeakNodeWrapper;
mod simple_node;

#[async_trait]
pub trait Node: 'static + Send + Sync + Eq + PartialEq + Hash {
    type Index: 'static + Send + Sync + Clone + Eq + PartialEq + Hash + Debug;
    type Data: 'static + Send + Sync + Clone + Debug;
    type Signal: 'static + Send + Sync + Debug;
    type NeighborNode: Node<Index = Self::Index, Data = Self::Data>;

    /// Returns id
    fn id(&self) -> Self::Index;

    /// Returns wrapped payload value of the node's content.
    /// Returns None if Node's content is not contained payload.
    async fn value(&self) -> Option<Self::Data>;

    /// Sets the value to the node's payload
    async fn set_value(&self, new_value: Self::Data) -> Option<Self::Data>;

    /// Returns children collection wrapped into RwLock guard
    fn children(&self) -> Arc<RwLock<HashSet<Arc<Self::NeighborNode>>>>;

    /// Returns parents collection wrapped into RwLock guard
    fn parents(&self) -> Arc<RwLock<HashSet<WeakNodeWrapper<Self::NeighborNode>>>>;

    /// Checks if the node has a child with the given id.
    async fn has_child_with_id(&self, id: &Self::Index) -> bool {
        let children_binding = self.children();
        let r_children = children_binding.read().await;
        r_children.iter().any(|child| child.id() == *id)
    }

    /// Checks if current node has children
    async fn has_children(&self) -> bool {
        let children_binding = self.children();
        let r_children = children_binding.read().await;
        r_children.len() > 0
    }

    /// Returns the child ids of the node.
    ///
    /// # Example
    ///
    /// ```text
    /// Cyclic graph :
    ///          1
    ///         /|\
    ///        / | \
    ///    -->2  6  8
    ///   /  /   |   \
    ///  /  /    |    \
    /// |  3<-   7     \
    /// \  | |  /|     |
    ///   -4 | / |    /
    ///    |  \| |   /
    ///     -->5 |  /
    ///        | | /
    ///         \|/
    ///          9
    ///
    /// 1 -> [2 -> [3 -> 4 -> [2, 5 -> [3, 9]]], 6 -> 7 -> [5, 9], 8 -> 9]
    ///
    /// for 1 is [2, 6, 8]
    /// for 2 is [3]
    /// for 3 is [4]
    /// for 4 is [2, 5]
    /// for 5 is [3, 9]
    /// for 6 is [7]
    /// for 7 is [5, 9]
    /// for 8 is [9]
    /// for 9 is []
    /// ```
    async fn child_ids(&self) -> Vec<Self::Index> {
        let children_binding = self.children();
        let r_children = children_binding.read().await;
        r_children.iter().map(|child| child.id()).collect()
    }

    /// Returns the successor ids of the node.
    ///
    /// # Example:
    ///
    /// ```text
    /// 1 -> [2 -> [3 -> 4 -> [2, 5 -> [3, 9]]], 6 -> 7 -> [5, 9], 8 -> 9]
    /// for 1 is [2, 3, 4, 5, 6, 7, 8, 9]
    /// for 2 is [2, 3, 4, 5, 9]
    /// for 3 is [4, 2, 3, 5, 9]
    /// for 4 is [2, 3, 4, 5, 9]
    /// for 5 is [3, 4, 2, 5, 9]
    /// for 6 is [7, 5, 3, 4, 2, 9]
    /// for 7 is [5, 3, 4, 2, 9]
    /// for 8 is [9]
    /// for 9 is []
    /// ```
    async fn successor_ids(&self, ids_set: &HashSet<Self::Index>) -> Vec<Self::Index> {
        let my_id = self.id();
        let mut children_ids = HashSet::<Self::Index>::new();
        children_ids.extend(ids_set.clone());
        let children_binding = self.children();
        let r_children = children_binding.read().await;

        for child in r_children.iter() {
            if my_id != child.id() && !ids_set.contains(&child.id()) {
                children_ids.insert(child.id().clone());
                children_ids.extend(child.successor_ids(&children_ids).await);
            }
        }
        children_ids.into_iter().collect()
    }

    /// Returns the predecessor ids of the node.
    ///
    /// # Example:
    ///
    /// ```text
    /// 1 -> [2 -> [3 -> 4 -> [2, 5 -> [3, 9]]], 6 -> 7 -> [5, 9], 8 -> 9]
    /// for 1 is []
    /// for 2 is [1, 2, 3, 4, 5, 6, 7]
    /// for 3 is [1, 2, 3, 4, 5, 6, 7]
    /// for 4 is [1, 2, 3, 4, 5, 6, 7]
    /// for 5 is [1, 2, 3, 4, 5, 6, 7]
    /// for 6 is [1]
    /// for 7 is [1, 6]
    /// for 8 is [1]
    /// for 9 is [1, 2, 3, 4, 5, 6, 7, 8]
    /// ```
    async fn predecessor_ids(&self, ids_set: &HashSet<Self::Index>) -> Vec<Self::Index> {
        let mut parents_ids = HashSet::<Self::Index>::new();
        parents_ids.extend(ids_set.clone());
        let my_id = self.id();
        let parents_binding = self.parents();
        let r_parents = parents_binding.read().await;

        for parent in r_parents
            .iter()
            .filter_map(|weak_parent| weak_parent.0.upgrade())
        {
            if my_id != parent.id() && !ids_set.contains(&parent.id()) {
                parents_ids.insert(parent.id().clone());
                parents_ids.extend(parent.predecessor_ids(&parents_ids).await);
            }
        }
        parents_ids.into_iter().collect()
    }

    /// Creates link to specified node as child
    async fn link_child(
        &self,
        new_child: Arc<Self::NeighborNode>,
    ) -> Result<bool, Error<Self::Index>> {
        if self.id() == new_child.id() {
            Err(Error::CannotLinkToItself)
        } else {
            let children_binding = self.children();
            let result = {
                let mut w_children = children_binding.write().await;

                w_children.insert(new_child.clone())
            };

            Ok(result)
        }
    }

    /// Creates link to specified node as parent.
    async fn link_parent(
        &self,
        new_parent: Arc<Self::NeighborNode>,
    ) -> Result<bool, Error<Self::Index>> {
        if self.id() == new_parent.id() {
            Err(Error::CannotLinkToItself)
        } else {
            let parents_binding = self.parents();
            let result = {
                let mut w_parents = parents_binding.write().await;

                let weak_parent_ref = WeakNodeWrapper(Arc::downgrade(&new_parent));
                w_parents.insert(weak_parent_ref)
            };
            Ok(result)
        }
    }

    /// Creates link to specified node as child synchronously
    fn try_link_child(
        &self,
        new_child: Arc<Self::NeighborNode>,
    ) -> Result<bool, Error<Self::Index>> {
        if self.id() == new_child.id() {
            return Err(Error::CannotLinkToItself);
        }
        let children_binding = self.children();
        children_binding
            .try_write()
            .map(|mut w_children| w_children.insert(new_child.clone()))
            .map_err(Error::from)
    }

    /// Created link to specified node as parent synchronously
    fn try_link_parent(
        &self,
        new_parent: Arc<Self::NeighborNode>,
    ) -> Result<bool, Error<Self::Index>> {
        if self.id() == new_parent.id() {
            Err(Error::CannotLinkToItself)
        } else {
            let parents_binding = self.parents();
            parents_binding
                .try_write()
                .map(|mut w_parents| {
                    let weak_parent_ref = WeakNodeWrapper(Arc::downgrade(&new_parent));
                    w_parents.insert(weak_parent_ref)
                })
                .map_err(Error::from)
        }
    }

    /// Remove link between child and current node.
    async fn unlink_child(
        &self,
        child: Arc<Self::NeighborNode>,
    ) -> Result<bool, Error<Self::Index>> {
        let children_binding = self.children();
        let mut children = children_binding.write().await;
        Ok(children.remove(&child.clone()))
    }

    /// Removes link between parent nad current node
    async fn unlink_parent(
        &'static self,
        parent: Arc<Self::NeighborNode>,
    ) -> Result<bool, Error<Self::Index>> {
        let parents_binding = self.parents();
        let mut w_parents = parents_binding.write().await;
        let weak_parent_ref = WeakNodeWrapper(Arc::downgrade(&parent));
        let res = w_parents.remove(&weak_parent_ref);

        let boxed_self = Box::pin(self);
        spawn(async move { boxed_self.clean_up_parents() });

        Ok(res)
    }

    async fn clean_up_parents(&self) {
        let parent_binding = self.parents();
        let mut w_parents = parent_binding.write().await;
        w_parents.retain(|weak_ref| weak_ref.0.strong_count() > 0);
    }

    /// Checks if current node has parent node specified by id
    async fn has_parent_with_id(&self, id: &Self::Index) -> bool {
        let parents_binding = self.parents();
        let r_parents = parents_binding.read().await;
        r_parents.iter().any(|weak_ref| match weak_ref.0.upgrade() {
            Some(parent) => parent.id() == *id,
            _ => false,
        })
    }

    /// checks if current mode has parents
    async fn has_parents(&self) -> bool {
        let parents_binding = self.parents();
        let r_parents = parents_binding.read().await;
        r_parents.len() > 0
    }
}

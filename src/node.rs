use std::{borrow::Borrow, collections::HashSet, hash::Hash, sync::Arc};

use tokio::sync::RwLock;

use crate::{content::Content, error::CyclicGraphError as CGError};

/// A node in a graph with a set of ancestor and descendant nodes, a unique identifier,
/// and a payload that it is associated with.
/// I - the node identifier type
/// D - the type of node payload data
/// S - the type of signal which can operate by inner struct in Node.data
#[derive(Debug)]
pub struct Node<I, D, S = ()> {
    /// The unique identifier
    id: I,

    /// The payload
    content: Arc<RwLock<dyn Content<I, D, S>>>,

    /// The parent ids set
    parent_ids: RwLock<HashSet<I>>,

    /// The child ids set
    child_ids: RwLock<HashSet<I>>,
}

impl<I, D: 'static, S: 'static> Node<I, D, S>
where
    I: Clone + Eq + PartialEq + Hash + 'static,
{
    /// The node constructor
    ///
    /// # Example
    ///
    /// ```
    /// use cyclic_graph::{Node, Error as CGError, Content};
    /// use std::sync::Arc;
    /// use tokio::sync::RwLock;
    /// use async_trait::async_trait;
    /// use std::any::Any;
    ///
    /// #[derive(Debug)]
    /// struct StringContent {
    ///     data: Arc<RwLock<String>>,
    /// }
    ///
    /// impl StringContent {
    ///     fn new(data: &str) -> Self {
    ///         Self {
    ///             data: Arc::new(RwLock::new(String::from(data))),
    ///         }
    ///     }
    /// }
    ///
    /// #[async_trait]
    /// impl Content<usize, String, ()> for StringContent {
    ///     fn data(&self) -> Arc<RwLock<String>> {
    ///         self.data.clone()
    ///     }
    ///
    ///     fn set_data(
    ///         &mut self,
    ///         data: Arc<RwLock<String>>
    ///     ) -> Result<Arc<RwLock<String>>, CGError<usize>> {
    ///         let prev = self.data.clone();
    ///         self.data = data.clone();
    ///         Ok(prev)
    ///     }
    ///
    ///     fn as_any(&self) -> &dyn Any {
    ///         self
    ///     }
    /// }
    ///
    /// #[derive(Debug)]
    /// struct VecUsizeContent {
    ///     data: Arc<RwLock<Vec<usize>>>,
    /// }
    ///
    /// impl VecUsizeContent {
    ///     fn new(data: &[usize]) -> Self {
    ///         Self {
    ///             data: Arc::new(RwLock::new(data.into())),
    ///         }
    ///     }
    /// }
    ///
    /// #[async_trait]
    /// impl Content<String, Vec<usize>, ()> for VecUsizeContent {
    ///     fn data(&self) -> Arc<RwLock<Vec<usize>>> {
    ///         self.data.clone()
    ///     }
    ///
    ///     fn set_data(
    ///         &mut self,
    ///         data: Arc<RwLock<Vec<usize>>>,
    ///     ) -> Result<Arc<RwLock<Vec<usize>>>, CGError<String>> {
    ///         let prev = self.data.clone();
    ///         self.data = data.clone();
    ///         Ok(prev)
    ///     }
    ///
    ///     fn as_any(&self) -> &dyn Any {
    ///         self
    ///     }
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let content = StringContent::new("one");
    ///     let node_i32 = Node::new(1, Arc::new(RwLock::new(content)));
    ///
    ///     let content = StringContent::new("zero");
    ///     let node_usize = Node::<usize, String, _>::new(0, Arc::new(RwLock::new(content)));
    ///
    ///     let content = VecUsizeContent::new(&[0_usize, 1, 2]);
    ///     let node_string_id = Node::<String, Vec<usize>, _>::new(
    ///         String::from("HL_0"),
    ///         Arc::new(RwLock::new(content)),
    ///     );
    ///
    ///     Ok(())
    /// }
    ///
    /// ```
    pub fn new(id: I, content: Arc<RwLock<dyn Content<I, D, S>>>) -> Self {
        Self {
            id,
            content,
            parent_ids: RwLock::new(HashSet::new()),
            child_ids: RwLock::new(HashSet::new()),
        }
    }

    /// Returns id reference
    pub fn id(&self) -> &I {
        &self.id
    }

    /// Returns wrapped payload data
    pub async fn data(&self) -> Arc<RwLock<D>> {
        self.content.read().await.data()
    }

    /// Changes wrapped payload data
    pub async fn set_data(&self, value: Arc<RwLock<D>>) -> Result<Arc<RwLock<D>>, CGError<I>> {
        self.content.write().await.set_data(value)
    }

    pub async fn link_nodes(
        &self,
        other: Arc<Node<I, D, S>>,
        self_ids: &RwLock<HashSet<I>>,
        other_ids: &RwLock<HashSet<I>>,
        is_parent_to_child: bool,
    ) -> Result<bool, CGError<I>> {
        if self.id == other.id {
            return Err(CGError::CannotLinkToItself);
        }
        let res = self_ids.write().await.insert(other.id().clone())
            && other_ids.write().await.insert(self.id().clone());
        dbg!(res);
        if res {
            if res {
                return if is_parent_to_child {
                    // When self - parent, other - child
                    other
                        .content
                        .read()
                        .await
                        .link_accept(self.content.clone())
                        .await
                } else {
                    // When self - child, other - parent
                    self.content
                        .read()
                        .await
                        .link_accept(other.content.clone())
                        .await
                };
            }
        }

        Ok(res)
    }

    /// Checks if current node has child node specified by id
    pub async fn has_child<Q>(&self, id: &Q) -> bool
    where
        I: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.child_ids.read().await.contains(id)
    }

    /// Checks if current node has connections to children
    pub async fn has_children(&self) -> bool {
        !self.child_ids.read().await.is_empty()
    }

    /// Returns vector with child ids
    pub async fn child_ids(&self) -> Vec<I> {
        self.child_ids
            .read()
            .await
            .iter()
            .cloned()
            .collect::<Vec<_>>()
    }

    /// Creates link to specified child and from child to current node as to parent
    pub async fn link_child(&self, child: Arc<Node<I, D, S>>) -> Result<bool, CGError<I>> {
        self.link_nodes(child.clone(), &self.child_ids, &child.parent_ids, true)
            .await
    }

    /// Creates link to specified child and from child to current node as to parent synchronously
    pub fn try_link_child(&self, child: Arc<Node<I, D, S>>) -> Result<bool, CGError<I>> {
        if self.id == child.id {
            return Err(CGError::CannotLinkToItself);
        }
        self.child_ids
            .try_write()
            .map(|mut src_child_ids| src_child_ids.insert(child.id().clone()))
            .and_then(|insert_result| {
                if !insert_result {
                    return Ok(insert_result);
                }
                child
                    .parent_ids
                    .try_write()
                    .map(|mut parent_ids| parent_ids.insert(self.id().clone()))
            })
            .map_err(CGError::from)
            .and_then(|insert_result| {
                if !insert_result {
                    return Ok(insert_result);
                }
                child
                    .content
                    .try_read()
                    .map_err(CGError::from)
                    .and_then(|acceptor_content| {
                        acceptor_content.try_link_accept(self.content.clone())
                    })
            })
    }

    /// Removes links between child and current node as parent
    pub async fn unlink_child(&self, child: Arc<Node<I, D, S>>) -> Result<bool, CGError<I>> {
        let res = child
            .content
            .read()
            .await
            .link_disconnect(self.content.clone())
            .await?;

        Ok(res
            && self.child_ids.write().await.remove(child.id())
            && child.parent_ids.write().await.remove(self.id()))
    }

    /// Creates link to specified parent and from parent to current node as to child
    pub async fn link_parent(&self, parent: Arc<Node<I, D, S>>) -> Result<bool, CGError<I>> {
        self.link_nodes(parent.clone(), &self.parent_ids, &parent.child_ids, false)
            .await
    }

    /// Creates link to specified parent and from parent to current node as to child synchronously
    pub fn try_link_parent(&self, parent: Arc<Node<I, D, S>>) -> Result<bool, CGError<I>> {
        if self.id == parent.id {
            return Err(CGError::CannotLinkToItself);
        }
        self.parent_ids
            .try_write()
            .map(|mut src_parent_ids| src_parent_ids.insert(parent.id().clone()))
            .and_then(|insert_result| {
                if !insert_result {
                    return Ok(insert_result);
                }
                parent
                    .child_ids
                    .try_write()
                    .map(|mut child_ids| child_ids.insert(self.id().clone()))
            })
            .map_err(CGError::from)
            .and_then(|insert_result| {
                if !insert_result {
                    return Ok(insert_result);
                }
                self.content
                    .try_read()
                    .map_err(CGError::from)
                    .and_then(|acceptor_content| {
                        acceptor_content.try_link_accept(parent.content.clone())
                    })
            })
    }

    /// Removes links between parent and current node as child
    pub async fn unlink_parent(&self, parent: Arc<Node<I, D, S>>) -> Result<bool, CGError<I>> {
        let res = self
            .content
            .read()
            .await
            .link_disconnect(parent.content.clone())
            .await?;

        Ok(res
            && self.parent_ids.write().await.remove(parent.id())
            && parent.child_ids.write().await.remove(self.id()))
    }

    /// Checks if current node has parent node specified by id
    pub async fn has_parent<Q>(&self, id: &Q) -> bool
    where
        I: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.parent_ids.read().await.contains(id)
    }

    /// Checks if current node has connections to parents
    pub async fn has_parents(&self) -> bool {
        !self.parent_ids.read().await.is_empty()
    }

    /// Returns vector with parent ids
    pub async fn parent_ids(&self) -> Vec<I> {
        self.parent_ids
            .read()
            .await
            .iter()
            .cloned()
            .collect::<Vec<_>>()
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    mod for_id_as_usize {
        use std::{any::Any, error::Error};

        use super::*;
        use async_trait::async_trait;

        #[derive(Debug)]
        struct StringContent {
            data: Arc<RwLock<String>>,
        }

        impl StringContent {
            pub fn new(data: &str) -> Self {
                Self {
                    data: Arc::new(RwLock::new(String::from(data))),
                }
            }
        }

        #[async_trait]
        impl Content<usize, String, ()> for StringContent {
            fn data(&self) -> Arc<RwLock<String>> {
                self.data.clone()
            }

            fn set_data(
                &mut self,
                data: Arc<RwLock<String>>,
            ) -> Result<Arc<RwLock<String>>, CGError<usize>> {
                let ret = self.data.clone();
                self.data = data.clone();
                Ok(ret)
            }

            fn as_any(&self) -> &dyn Any {
                self
            }
        }

        #[derive(Debug)]
        struct VecUsizeContent {
            data: Arc<RwLock<Vec<usize>>>,
        }

        impl VecUsizeContent {
            pub fn new(data: &[usize]) -> Self {
                Self {
                    data: Arc::new(RwLock::new(data.into())),
                }
            }
        }

        #[async_trait]
        impl Content<usize, Vec<usize>, ()> for VecUsizeContent {
            fn data(&self) -> Arc<RwLock<Vec<usize>>> {
                self.data.clone()
            }

            fn set_data(
                &mut self,
                data: Arc<RwLock<Vec<usize>>>,
            ) -> Result<Arc<RwLock<Vec<usize>>>, CGError<usize>> {
                let prev = self.data.clone();
                self.data = data.clone();
                Ok(prev)
            }

            fn as_any(&self) -> &dyn Any {
                self
            }
        }

        #[tokio::test]
        async fn test_create_new_node() {
            let content = StringContent::new("test");
            let node = Node::new(0_usize, Arc::new(RwLock::new(content)));

            let content = VecUsizeContent::new(&[1_usize, 2, 3]);
            let vec_node = Node::new(1_usize, Arc::new(RwLock::new(content)));

            assert_eq!(node.id, 0);
            assert_eq!(node.data().await.read().await.clone(), "test");
            assert!(node.parent_ids.read().await.is_empty());
            assert!(node.child_ids.read().await.is_empty());

            assert_eq!(vec_node.id, 1);
            assert_eq!(vec_node.data().await.read().await.len(), 3);
        }

        #[test]
        fn test_id_accessor_should_return_correct_id_value() {
            let content = StringContent::new("test");
            let node = Node::new(0_usize, Arc::new(RwLock::new(content)));

            assert_eq!(node.id(), &0);
        }

        #[tokio::test]
        async fn test_data_accessor_should_return_correct_data_ref() {
            let content = StringContent::new("test");
            let node = Node::new(0_usize, Arc::new(RwLock::new(content)));

            assert_eq!(node.data().await.read().await.clone(), "test");
        }

        #[tokio::test]
        async fn test_set_data_allowed_to_change_node_data() -> Result<(), Box<dyn Error>> {
            let content = StringContent::new("test");
            let node = Node::new(0_usize, Arc::new(RwLock::new(content)));

            let old_data = node
                .set_data(Arc::new(RwLock::new("new test".into())))
                .await?;

            assert_eq!(old_data.read().await.clone(), "test");
            assert_eq!(node.data().await.read().await.clone(), "new test");

            Ok(())
        }

        #[tokio::test]
        async fn test_data_allowed_change_node_data_by_mutation_node_data() {
            let content = StringContent::new("test");
            let node = Node::new(0_usize, Arc::new(RwLock::new(content)));

            {
                let binding = node.data().await;
                let mut w_data = binding.write().await;
                *w_data = "new test".into();
            }

            assert_eq!(node.data().await.read().await.clone(), "new test");
        }

        #[tokio::test]
        async fn test_linking_two_nodes_by_link_child() -> Result<(), Box<dyn Error>> {
            let content = StringContent::new("root");
            let root_node = Arc::new(Node::new(0_usize, Arc::new(RwLock::new(content))));

            let content = StringContent::new("child");
            let child_node = Arc::new(Node::new(1_usize, Arc::new(RwLock::new(content))));

            let res = root_node.link_child(child_node.clone()).await?;
            assert!(res);

            assert!(!root_node.has_parents().await);
            assert!(root_node.has_children().await);
            assert!(child_node.has_parents().await);
            assert!(!child_node.has_children().await);

            Ok(())
        }

        #[tokio::test]
        async fn test_linking_two_nodes_by_try_link_child() {
            let content = StringContent::new("root");
            let root_node = Arc::new(Node::new(0_usize, Arc::new(RwLock::new(content))));

            let content = StringContent::new("child");
            let child_node = Arc::new(Node::new(1_usize, Arc::new(RwLock::new(content))));

            let op_res = root_node.try_link_child(child_node.clone());
            assert!(op_res.is_ok());
            assert!(op_res.unwrap());

            assert!(!root_node.has_parents().await);
            assert!(root_node.has_children().await);
            assert!(child_node.has_parents().await);
            assert!(!child_node.has_children().await);
        }

        #[tokio::test]
        async fn test_second_link_child_attempt_should_return_false() -> Result<(), Box<dyn Error>>
        {
            let parent = Arc::new(Node::new(
                0_usize,
                Arc::new(RwLock::new(StringContent::new("parent"))),
            ));
            let child = Arc::new(Node::new(
                1_usize,
                Arc::new(RwLock::new(StringContent::new("child"))),
            ));

            assert!(parent.link_child(child.clone()).await?);
            assert!(!parent.link_child(child.clone()).await?);

            Ok(())
        }

        #[test]
        fn test_second_try_link_child_attempt_should_return_false() -> Result<(), Box<dyn Error>> {
            let parent = Arc::new(Node::new(
                0_usize,
                Arc::new(RwLock::new(StringContent::new("parent"))),
            ));
            let child = Arc::new(Node::new(
                1_usize,
                Arc::new(RwLock::new(StringContent::new("child"))),
            ));

            assert!(parent.try_link_child(child.clone())?);
            assert!(!parent.try_link_child(child.clone())?);

            Ok(())
        }

        #[tokio::test]
        async fn test_second_link_parent_attempt_should_return_false() -> Result<(), Box<dyn Error>>
        {
            let parent = Arc::new(Node::new(
                0_usize,
                Arc::new(RwLock::new(StringContent::new("parent"))),
            ));
            let child = Arc::new(Node::new(
                1_usize,
                Arc::new(RwLock::new(StringContent::new("child"))),
            ));

            assert!(child.link_parent(parent.clone()).await?);
            assert!(!child.link_parent(parent.clone()).await?);

            Ok(())
        }

        #[tokio::test]
        async fn test_unlink_child_should_break_link_between_linked_nodes()
        -> Result<(), Box<dyn Error>> {
            let parent = Arc::new(Node::new(
                0_usize,
                Arc::new(RwLock::new(StringContent::new("parent"))),
            ));
            let child = Arc::new(Node::new(
                1_usize,
                Arc::new(RwLock::new(StringContent::new("child"))),
            ));

            assert!(parent.link_child(child.clone()).await?);

            assert!(parent.unlink_child(child.clone()).await?);

            assert!(!parent.has_children().await);
            assert!(!child.has_parents().await);

            Ok(())
        }

        #[tokio::test]
        async fn test_second_unlink_child_should_return_false() -> Result<(), Box<dyn Error>> {
            let parent = Arc::new(Node::new(
                0_usize,
                Arc::new(RwLock::new(StringContent::new("parent"))),
            ));
            let child = Arc::new(Node::new(
                1_usize,
                Arc::new(RwLock::new(StringContent::new("child"))),
            ));

            assert!(parent.link_child(child.clone()).await?);

            assert!(parent.unlink_child(child.clone()).await?);
            assert!(!parent.unlink_child(child.clone()).await?);

            Ok(())
        }

        #[tokio::test]
        async fn test_unlink_parent_should_break_link_between_linked_nodes()
        -> Result<(), Box<dyn Error>> {
            let parent = Arc::new(Node::new(
                0_usize,
                Arc::new(RwLock::new(StringContent::new("parent"))),
            ));
            let child = Arc::new(Node::new(
                1_usize,
                Arc::new(RwLock::new(StringContent::new("child"))),
            ));

            assert!(parent.link_child(child.clone()).await?);

            assert!(child.unlink_parent(parent.clone()).await?);

            assert!(!parent.has_children().await);
            assert!(!child.has_parents().await);

            Ok(())
        }

        #[tokio::test]
        async fn test_second_unlink_parent_should_return_false() -> Result<(), Box<dyn Error>> {
            let parent = Arc::new(Node::new(
                0_usize,
                Arc::new(RwLock::new(StringContent::new("parent"))),
            ));
            let child = Arc::new(Node::new(
                1_usize,
                Arc::new(RwLock::new(StringContent::new("child"))),
            ));

            assert!(child.link_parent(parent.clone()).await?);

            assert!(child.unlink_parent(parent.clone()).await?);
            assert!(!child.unlink_parent(parent.clone()).await?);

            Ok(())
        }

        #[tokio::test]
        async fn test_child_ids_should_return_children_identifiers_in_vector()
        -> Result<(), Box<dyn Error>> {
            let parent = Arc::new(Node::new(
                0_usize,
                Arc::new(RwLock::new(StringContent::new("parent"))),
            ));

            let child1 = Arc::new(Node::new(
                1_usize,
                Arc::new(RwLock::new(StringContent::new("child1"))),
            ));
            let child2 = Arc::new(Node::new(
                2_usize,
                Arc::new(RwLock::new(StringContent::new("child2"))),
            ));

            assert!(parent.link_child(child1.clone()).await?);
            assert!(parent.link_child(child2.clone()).await?);

            let ids = parent.child_ids().await;

            assert!(!ids.is_empty());
            assert_eq!(ids.len(), 2);
            assert!(ids.contains(&1));
            assert!(ids.contains(&2));

            Ok(())
        }

        #[tokio::test]
        async fn test_children_ids_should_return_children_identifiers_in_vector()
        -> Result<(), Box<dyn Error>> {
            let parent1 = Arc::new(Node::new(
                0_usize,
                Arc::new(RwLock::new(StringContent::new("parent1"))),
            ));
            let parent2 = Arc::new(Node::new(
                1_usize,
                Arc::new(RwLock::new(StringContent::new("parent2"))),
            ));

            let child = Arc::new(Node::new(
                2_usize,
                Arc::new(RwLock::new(StringContent::new("child1"))),
            ));

            assert!(parent1.link_child(child.clone()).await?);
            assert!(parent2.link_child(child.clone()).await?);

            let ids = child.parent_ids().await;

            assert!(!ids.is_empty());
            assert_eq!(ids.len(), 2);
            assert!(ids.contains(&0));
            assert!(ids.contains(&1));

            Ok(())
        }

        #[tokio::test]
        async fn test_linking_two_nodes_by_link_parent() -> Result<(), Box<dyn Error>> {
            let root_node = Arc::new(Node::<_, _, ()>::new(
                0_usize,
                Arc::new(RwLock::new(StringContent::new("root"))),
            ));
            let child_node = Arc::new(Node::new(
                1_usize,
                Arc::new(RwLock::new(StringContent::new("child"))),
            ));

            assert!(child_node.link_parent(root_node.clone()).await?);

            assert!(!root_node.has_parents().await);
            assert!(root_node.has_children().await);
            assert!(child_node.has_parents().await);
            assert!(!child_node.has_children().await);

            Ok(())
        }

        #[tokio::test]
        async fn test_has_child_should_return_true_with_correct_child_id()
        -> Result<(), Box<dyn Error>> {
            let node0 = Arc::new(Node::new(
                0_usize,
                Arc::new(RwLock::new(StringContent::new("root"))),
            ));
            let node1 = Arc::new(Node::new(
                1_usize,
                Arc::new(RwLock::new(StringContent::new("child"))),
            ));

            assert!(node0.link_child(node1.clone()).await?);

            assert!(node0.has_child(&1).await);

            Ok(())
        }

        #[tokio::test]
        async fn test_has_child_should_return_false_with_incorrect_child_id() {
            let node = Node::new(0_usize, Arc::new(RwLock::new(StringContent::new("test"))));

            assert!(!node.has_child(&1).await);
        }

        #[tokio::test]
        async fn test_has_parent_should_return_true_with_correct_parent_id()
        -> Result<(), Box<dyn Error>> {
            let node0 = Arc::new(Node::new(
                0_usize,
                Arc::new(RwLock::new(StringContent::new("n0"))),
            ));
            let node1 = Arc::new(Node::new(
                1_usize,
                Arc::new(RwLock::new(StringContent::new("n1"))),
            ));

            assert!(node1.link_parent(node0.clone()).await?);

            assert!(node1.has_parent(&0).await);

            Ok(())
        }

        #[tokio::test]
        async fn test_has_parent_should_return_false_with_incorrect_parent_id() {
            let node = Node::new(0_usize, Arc::new(RwLock::new(StringContent::new("test"))));

            assert!(!node.has_parent(&1).await);
        }

        #[tokio::test]
        async fn test_not_allow_link_self_node_as_child() {
            let node = Arc::new(Node::new(
                0_usize,
                Arc::new(RwLock::new(StringContent::new("test"))),
            ));

            assert!(node.link_child(node.clone()).await.is_err());
        }

        #[tokio::test]
        async fn test_allow_cyclic_links_between_two_nodes() -> Result<(), Box<dyn Error>> {
            let node0 = Arc::new(Node::new(
                0_usize,
                Arc::new(RwLock::new(StringContent::new("test"))),
            ));
            let node1 = Arc::new(Node::new(
                1,
                Arc::new(RwLock::new(StringContent::new("test"))),
            ));

            assert!(node0.link_child(node1.clone()).await?);
            assert!(node1.link_child(node0.clone()).await?);

            assert!(node0.has_child(node1.id()).await);
            assert!(node1.has_child(node0.id()).await);

            assert!(node0.has_parent(node1.id()).await);
            assert!(node1.has_parent(node0.id()).await);

            Ok(())
        }
    }

    mod for_id_as_str {
        use super::*;
        use async_trait::async_trait;
        use std::{any::Any, error::Error};

        #[derive(Debug)]
        struct StringContent {
            data: Arc<RwLock<String>>,
        }

        impl StringContent {
            pub fn new(data: &str) -> Self {
                Self {
                    data: Arc::new(RwLock::new(String::from(data))),
                }
            }
        }

        #[async_trait]
        impl Content<&'static str, String, ()> for StringContent {
            fn data(&self) -> Arc<RwLock<String>> {
                self.data.clone()
            }

            fn set_data(
                &mut self,
                data: Arc<RwLock<String>>,
            ) -> Result<Arc<RwLock<String>>, CGError<&'static str>> {
                let ret = self.data.clone();
                self.data = data.clone();
                Ok(ret)
            }

            fn as_any(&self) -> &dyn Any {
                self
            }
        }

        #[derive(Debug)]
        struct VecUsizeContent {
            data: Arc<RwLock<Vec<usize>>>,
        }

        impl VecUsizeContent {
            pub fn new(data: &[usize]) -> Self {
                Self {
                    data: Arc::new(RwLock::new(data.into())),
                }
            }
        }

        #[async_trait]
        impl Content<&'static str, Vec<usize>, ()> for VecUsizeContent {
            fn data(&self) -> Arc<RwLock<Vec<usize>>> {
                self.data.clone()
            }

            fn set_data(
                &mut self,
                data: Arc<RwLock<Vec<usize>>>,
            ) -> Result<Arc<RwLock<Vec<usize>>>, CGError<&'static str>> {
                let prev = self.data.clone();
                self.data = data.clone();
                Ok(prev)
            }

            fn as_any(&self) -> &dyn Any {
                self
            }
        }

        #[tokio::test]
        async fn test_create_new_node() {
            let node = Node::new("IL", Arc::new(RwLock::new(StringContent::new("test"))));
            let vec_node = Node::new(
                "IL",
                Arc::new(RwLock::new(VecUsizeContent::new(&[1_usize, 2, 3]))),
            );

            assert_eq!(node.id, "IL");
            assert_eq!(node.data().await.read().await.clone(), "test");
            assert!(node.parent_ids.read().await.is_empty());
            assert!(node.child_ids.read().await.is_empty());

            assert_eq!(vec_node.id, "IL");
            assert_eq!(vec_node.data().await.read().await.len(), 3);
        }

        #[test]
        fn test_id_accessor_should_return_correct_id_value() {
            let node = Node::new("IL", Arc::new(RwLock::new(StringContent::new("test"))));

            assert_eq!(node.id(), &"IL");
        }

        #[tokio::test]
        async fn test_data_accessor_should_return_correct_data_ref() {
            let node = Node::new("IL", Arc::new(RwLock::new(StringContent::new("test"))));

            assert_eq!(node.data().await.read().await.clone(), "test");
        }

        #[tokio::test]
        async fn test_data_mut_accessor_allowed_to_change_node_data() {
            let node = Node::new("IL", Arc::new(RwLock::new(StringContent::new("test"))));

            {
                let data = node.data().await;
                let mut data_mut = data.write().await;
                *data_mut = "new test".into();
            }

            assert_eq!(node.data().await.read().await.clone(), "new test");
        }

        #[tokio::test]
        async fn test_set_data_should_change_node_data() -> Result<(), Box<dyn Error>> {
            let node = Node::new("IL", Arc::new(RwLock::new(StringContent::new("test"))));

            let prev_data = node
                .set_data(Arc::new(RwLock::new("new test".into())))
                .await?;

            assert_eq!(prev_data.read().await.clone(), "test");
            assert_eq!(node.data().await.read().await.clone(), "new test");

            Ok(())
        }

        #[tokio::test]
        async fn test_linking_two_nodes_by_link_child() -> Result<(), Box<dyn Error>> {
            let root_node = Arc::new(Node::new(
                "n0",
                Arc::new(RwLock::new(StringContent::new("root"))),
            ));
            let child_node = Arc::new(Node::new(
                "n1",
                Arc::new(RwLock::new(StringContent::new("child"))),
            ));

            assert!(root_node.link_child(child_node.clone()).await?);

            assert!(!root_node.has_parents().await);
            assert!(root_node.has_children().await);
            assert!(child_node.has_parents().await);
            assert!(!child_node.has_children().await);

            Ok(())
        }

        #[tokio::test]
        async fn test_second_link_child_attempt_should_return_false() -> Result<(), Box<dyn Error>>
        {
            let parent = Arc::new(Node::new(
                "n0",
                Arc::new(RwLock::new(StringContent::new("parent"))),
            ));
            let child = Arc::new(Node::new(
                "n1",
                Arc::new(RwLock::new(StringContent::new("child"))),
            ));

            assert!(parent.link_child(child.clone()).await?);
            assert!(!parent.link_child(child.clone()).await?);

            Ok(())
        }

        #[tokio::test]
        async fn test_second_link_parent_attempt_should_return_false() -> Result<(), Box<dyn Error>>
        {
            let parent = Arc::new(Node::new(
                "n0",
                Arc::new(RwLock::new(StringContent::new("parent"))),
            ));
            let child = Arc::new(Node::new(
                "n1",
                Arc::new(RwLock::new(StringContent::new("child"))),
            ));

            assert!(child.link_parent(parent.clone()).await?);
            assert!(!child.link_parent(parent.clone()).await?);

            Ok(())
        }

        #[tokio::test]
        async fn test_unlink_child_should_break_link_between_linked_nodes()
        -> Result<(), Box<dyn Error>> {
            let parent = Arc::new(Node::new(
                "n0",
                Arc::new(RwLock::new(StringContent::new("parent"))),
            ));
            let child = Arc::new(Node::new(
                "n1",
                Arc::new(RwLock::new(StringContent::new("child"))),
            ));

            assert!(parent.link_child(child.clone()).await?);

            assert!(parent.unlink_child(child.clone()).await?);

            assert!(!parent.has_children().await);
            assert!(!child.has_parents().await);

            Ok(())
        }

        #[tokio::test]
        async fn test_second_unlink_child_should_return_false() -> Result<(), Box<dyn Error>> {
            let parent = Arc::new(Node::new(
                "n0",
                Arc::new(RwLock::new(StringContent::new("parent"))),
            ));
            let child = Arc::new(Node::new(
                "n1",
                Arc::new(RwLock::new(StringContent::new("child"))),
            ));

            assert!(parent.link_child(child.clone()).await?);

            assert!(parent.unlink_child(child.clone()).await?);
            assert!(!parent.unlink_child(child.clone()).await?);

            assert!(!parent.has_children().await);
            assert!(!child.has_parents().await);

            Ok(())
        }

        #[tokio::test]
        async fn test_unlink_parent_should_break_link_between_linked_nodes()
        -> Result<(), Box<dyn Error>> {
            let parent = Arc::new(Node::new(
                "n0",
                Arc::new(RwLock::new(StringContent::new("parent"))),
            ));
            let child = Arc::new(Node::new(
                "n1",
                Arc::new(RwLock::new(StringContent::new("child"))),
            ));

            assert!(parent.link_child(child.clone()).await?);

            assert!(child.unlink_parent(parent.clone()).await?);

            assert!(!parent.has_children().await);
            assert!(!child.has_parents().await);

            Ok(())
        }

        #[tokio::test]
        async fn test_second_unlink_parent_should_return_false() -> Result<(), Box<dyn Error>> {
            let parent = Arc::new(Node::new(
                "n0",
                Arc::new(RwLock::new(StringContent::new("parent"))),
            ));
            let child = Arc::new(Node::new(
                "n1",
                Arc::new(RwLock::new(StringContent::new("child"))),
            ));

            assert!(child.link_parent(parent.clone()).await?);

            assert!(child.unlink_parent(parent.clone()).await?);
            assert!(!child.unlink_parent(parent.clone()).await?);

            Ok(())
        }

        #[tokio::test]
        async fn test_child_ids_should_return_children_identifiers_in_vector()
        -> Result<(), Box<dyn Error>> {
            let parent = Arc::new(Node::new(
                "n0",
                Arc::new(RwLock::new(StringContent::new("parent"))),
            ));

            let child1 = Arc::new(Node::new(
                "n1",
                Arc::new(RwLock::new(StringContent::new("child1"))),
            ));
            let child2 = Arc::new(Node::new(
                "n2",
                Arc::new(RwLock::new(StringContent::new("child2"))),
            ));

            assert!(parent.link_child(child1.clone()).await?);
            assert!(parent.link_child(child2.clone()).await?);

            let ids = parent.child_ids().await;

            assert!(!ids.is_empty());
            assert_eq!(ids.len(), 2);
            assert!(ids.contains(&"n1"));
            assert!(ids.contains(&"n2"));

            Ok(())
        }

        #[tokio::test]
        async fn test_children_ids_should_return_children_identifiers_in_vector()
        -> Result<(), Box<dyn Error>> {
            let parent1 = Arc::new(Node::new(
                "n0",
                Arc::new(RwLock::new(StringContent::new("parent1"))),
            ));
            let parent2 = Arc::new(Node::new(
                "n1",
                Arc::new(RwLock::new(StringContent::new("parent2"))),
            ));

            let child = Arc::new(Node::new(
                "n2",
                Arc::new(RwLock::new(StringContent::new("child"))),
            ));

            assert!(parent1.link_child(child.clone()).await?);
            assert!(parent2.link_child(child.clone()).await?);

            let ids = child.parent_ids().await;

            assert!(!ids.is_empty());
            assert_eq!(ids.len(), 2);
            assert!(ids.contains(&"n0"));
            assert!(ids.contains(&"n1"));

            Ok(())
        }

        #[tokio::test]
        async fn test_linking_two_nodes_by_link_parent() -> Result<(), Box<dyn Error>> {
            let root_node = Arc::new(Node::new(
                "n0",
                Arc::new(RwLock::new(StringContent::new("root"))),
            ));
            let child_node = Arc::new(Node::new(
                "n1",
                Arc::new(RwLock::new(StringContent::new("child"))),
            ));

            assert!(child_node.link_parent(root_node.clone()).await?);

            assert!(!root_node.has_parents().await);
            assert!(root_node.has_children().await);
            assert!(child_node.has_parents().await);
            assert!(!child_node.has_children().await);

            Ok(())
        }

        #[tokio::test]
        async fn test_has_child_should_return_true_with_correct_child_id()
        -> Result<(), Box<dyn Error>> {
            let node0 = Arc::new(Node::new(
                "n0",
                Arc::new(RwLock::new(StringContent::new("root"))),
            ));
            let node1 = Arc::new(Node::new(
                "n1",
                Arc::new(RwLock::new(StringContent::new("child"))),
            ));

            assert!(node0.link_child(node1.clone()).await?);

            assert!(node0.has_child(&"n1").await);

            Ok(())
        }

        #[tokio::test]
        async fn test_has_child_should_return_false_with_incorrect_child_id() {
            let node = Node::new("n0", Arc::new(RwLock::new(StringContent::new("test"))));

            assert!(!node.has_child(&"n1").await);
        }

        #[tokio::test]
        async fn test_has_parent_should_return_true_with_correct_parent_id()
        -> Result<(), Box<dyn Error>> {
            let node0 = Arc::new(Node::new(
                "n0",
                Arc::new(RwLock::new(StringContent::new("n0"))),
            ));
            let node1 = Arc::new(Node::new(
                "n1",
                Arc::new(RwLock::new(StringContent::new("n1"))),
            ));

            assert!(node1.link_parent(node0.clone()).await?);

            assert!(node1.has_parent(&"n0").await);

            Ok(())
        }

        #[tokio::test]
        async fn test_has_parent_should_return_false_with_incorrect_parent_id() {
            let node = Node::new("n0", Arc::new(RwLock::new(StringContent::new("test"))));

            assert!(!node.has_parent(&"n1").await);
        }

        #[tokio::test]
        async fn test_not_allow_link_self_node_as_child() {
            let node = Arc::new(Node::new(
                "n0",
                Arc::new(RwLock::new(StringContent::new("test"))),
            ));

            assert!(node.link_child(node.clone()).await.is_err());
        }

        #[tokio::test]
        async fn test_allow_cyclic_links_between_two_nodes() -> Result<(), Box<dyn Error>> {
            let node0 = Arc::new(Node::new(
                "n0",
                Arc::new(RwLock::new(StringContent::new("test0"))),
            ));
            let node1 = Arc::new(Node::new(
                "n1",
                Arc::new(RwLock::new(StringContent::new("test1"))),
            ));

            assert!(node0.link_child(node1.clone()).await?);
            assert!(node1.link_child(node0.clone()).await?);

            assert!(node0.has_child(node1.id()).await);
            assert!(node1.has_child(node0.id()).await);

            assert!(node0.has_parent(node1.id()).await);
            assert!(node1.has_parent(node0.id()).await);

            Ok(())
        }
    }

    mod for_struct_with_inner_connected_cells {
        use super::*;

        use std::{
            any::Any,
            collections::{HashMap, hash_map::Entry},
            error::Error,
            sync::Arc,
        };

        use async_trait::async_trait;
        use tokio::sync::{RwLock, broadcast};

        use crate::{Content, Error as CGError};

        type Cellular = HashMap<usize, InnerCell>;

        const CELLULAR_CAPACITY: usize = 10;

        #[derive(Debug)]
        struct InnerCell {
            inputs: HashMap<usize, broadcast::Receiver<u8>>,
            output: broadcast::Sender<u8>,
        }

        impl InnerCell {
            fn new(tx: broadcast::Sender<u8>) -> Self {
                Self {
                    inputs: HashMap::new(),
                    output: tx,
                }
            }
        }

        #[derive(Debug)]
        struct CellularContent {
            data: Arc<RwLock<Cellular>>,
            node_id: usize,
        }

        impl CellularContent {
            fn new(size: usize, node_id: usize) -> Self {
                if size > CELLULAR_CAPACITY {
                    panic!("The size should be less then {}", CELLULAR_CAPACITY)
                };
                let cell_group_id_prefix = node_id * CELLULAR_CAPACITY;
                let mut cellulars = HashMap::with_capacity(size);
                for id in 0..size {
                    let (tx, _) = broadcast::channel::<u8>(2);
                    cellulars.insert(cell_group_id_prefix + id, InnerCell::new(tx));
                }
                Self {
                    data: Arc::new(RwLock::new(cellulars)),
                    node_id,
                }
            }

            async fn has_incoming_channels(&self) -> bool {
                self.data
                    .read()
                    .await
                    .values()
                    .any(|item| item.inputs.len() > 0)
            }

            async fn count_incoming_channels(&self) -> usize {
                self.data
                    .read()
                    .await
                    .values()
                    .fold(0_usize, |acc, item| acc + item.inputs.len())
            }

            async fn count_outgoing_channels(&self) -> usize {
                self.data
                    .read()
                    .await
                    .values()
                    .fold(0_usize, |acc, item| acc + item.output.receiver_count())
            }
        }

        #[async_trait]
        impl Content<usize, Cellular, u8> for CellularContent {
            fn as_any(&self) -> &dyn Any {
                self
            }

            fn data(&self) -> Arc<RwLock<Cellular>> {
                self.data.clone()
            }

            fn set_data(
                &mut self,
                data: Arc<RwLock<Cellular>>,
            ) -> Result<Arc<RwLock<Cellular>>, CGError<usize>> {
                let prev = self.data.clone();
                self.data = data.clone();
                Ok(prev)
            }

            async fn provide_receiver(
                &self,
                src_idx: usize,
            ) -> Result<Option<broadcast::Receiver<u8>>, CGError<usize>> {
                match self.data.write().await.entry(src_idx) {
                    Entry::Occupied(entry) => Ok(Some(entry.get().output.subscribe())),
                    Entry::Vacant(_) => Err(CGError::LinksProviderHandlerError(format!(
                        "Cell with id {} not found at provider content",
                        src_idx
                    ))),
                }
            }

            fn try_provide_receiver(
                &self,
                src_idx: usize,
            ) -> Result<Option<broadcast::Receiver<u8>>, CGError<usize>> {
                match self.data.try_write()?.entry(src_idx) {
                    Entry::Occupied(entry) => Ok(Some(entry.get().output.subscribe())),
                    Entry::Vacant(_) => Err(CGError::LinksProviderHandlerError(format!(
                        "Cell with id {} not found at provider content",
                        src_idx
                    ))),
                }
            }

            async fn provide_src_ids(&self) -> Vec<usize> {
                self.data
                    .read()
                    .await
                    .keys()
                    .cloned()
                    .collect::<Vec<usize>>()
            }

            fn try_provide_src_ids(&self) -> Result<Vec<usize>, CGError<usize>> {
                let ids = self
                    .data
                    .try_read()?
                    .keys()
                    .cloned()
                    .collect::<Vec<usize>>();
                Ok(ids)
            }

            async fn link_accept(
                &self,
                provider: Arc<RwLock<dyn Content<usize, Cellular, u8> + Send + Sync>>,
            ) -> Result<bool, CGError<usize>> {
                let mut result = true;
                let src_ids = provider.read().await.provide_src_ids().await;
                let mut w_dst_data = self.data.write().await;
                for dst_content in w_dst_data.values_mut() {
                    for src_idx in &src_ids {
                        let rx = provider
                            .read()
                            .await
                            .provide_receiver(src_idx.clone())
                            .await?;
                        if let Some(rx) = rx {
                            if let Entry::Vacant(dst_entry) = dst_content.inputs.entry(*src_idx) {
                                dst_entry.insert(rx);
                                result &= true;
                            } else {
                                result &= false;
                            }
                        } else {
                            result &= false;
                        }
                    }
                }
                Ok(result)
            }

            fn try_link_accept(
                &self,
                provider: Arc<RwLock<dyn Content<usize, Cellular, u8> + Send + Sync>>,
            ) -> Result<bool, CGError<usize>> {
                let mut result = true;
                let src_ids = provider.try_read()?.try_provide_src_ids()?;
                for dst_content in self.data.try_write()?.values_mut() {
                    for src_idx in &src_ids {
                        let rx = provider.try_read()?.try_provide_receiver(src_idx.clone())?;
                        if let Some(rx) = rx {
                            if let Entry::Vacant(dst_entry) = dst_content.inputs.entry(*src_idx) {
                                dst_entry.insert(rx);
                                result &= true;
                            } else {
                                result &= false;
                            }
                        } else {
                            result &= false;
                        }
                    }
                }
                Ok(result)
            }

            async fn link_disconnect(
                &self,
                provider: Arc<RwLock<dyn Content<usize, Cellular, u8> + Send + Sync>>,
            ) -> Result<bool, CGError<usize>> {
                let mut result = true;
                let src_ids = provider.read().await.provide_src_ids().await;
                let mut w_dst_data = self.data.write().await;
                for dst_content in w_dst_data.values_mut() {
                    for src_idx in &src_ids {
                        match dst_content.inputs.entry(*src_idx) {
                            Entry::Occupied(dst_entry) => {
                                dst_entry.remove();
                                result &= true;
                            }
                            Entry::Vacant(_) => result &= false,
                        }
                    }
                }

                Ok(result)
            }

            fn try_link_disconnect(
                &self,
                provider: Arc<RwLock<dyn Content<usize, Cellular, u8> + Send + Sync>>,
            ) -> Result<bool, CGError<usize>> {
                let mut result = true;
                let src_ids = provider.try_read()?.try_provide_src_ids()?;
                for dst_content in self.data.try_write()?.values_mut() {
                    for src_idx in &src_ids {
                        match dst_content.inputs.entry(*src_idx) {
                            Entry::Occupied(dst_entry) => {
                                dst_entry.remove();
                                result &= true;
                            }
                            Entry::Vacant(_) => result &= false,
                        }
                    }
                }

                Ok(result)
            }
        }

        #[tokio::test]
        async fn test_create_new_node() {
            let node_id = 0;
            let content = CellularContent::new(3, node_id);
            let node: Node<usize, Cellular, u8> =
                Node::new(node_id, Arc::new(RwLock::new(content)));

            assert_eq!(node.id, 0);

            let binding = node.data().await;
            let data = binding.read().await;
            assert_eq!(data.capacity(), 3);

            assert!(!node.has_parents().await);
            assert!(!node.has_children().await);
        }

        #[test]
        fn test_id_accessor_should_return_correct_id_value() {
            let node_id = 0;
            let content = CellularContent::new(3, node_id);
            let node = Node::new(node_id, Arc::new(RwLock::new(content)));

            assert_eq!(node.id(), &0);
        }

        #[tokio::test]
        async fn test_data_accessor_should_return_correct_data_ref() {
            let node_id = 0;
            let content = CellularContent::new(2, node_id);
            let node: Node<usize, Cellular, u8> =
                Node::new(node_id, Arc::new(RwLock::new(content)));

            assert_eq!(node.data().await.read().await.len(), 2);
        }

        #[tokio::test]
        async fn test_linking_two_nodes_by_link_child() -> Result<(), Box<dyn Error>> {
            let node_id = 0;
            let content = CellularContent::new(3, node_id);
            let root_node = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            let node_id = 1;
            let content = CellularContent::new(2, node_id);
            let child_node: Arc<Node<usize, Cellular, u8>> =
                Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            let res = root_node.link_child(child_node.clone()).await?;
            assert!(res);

            assert!(!root_node.has_parents().await);
            assert!(root_node.has_children().await);
            assert!(child_node.has_parents().await);
            assert!(!child_node.has_children().await);

            let dyn_root_content = root_node.content.read().await;
            let concrete_root_content = dyn_root_content
                .as_any()
                .downcast_ref::<CellularContent>()
                .unwrap();

            assert!(!concrete_root_content.has_incoming_channels().await);
            assert_eq!(concrete_root_content.count_incoming_channels().await, 0);

            let dyn_child_content = child_node.content.read().await;
            let concrete_child_content = dyn_child_content
                .as_any()
                .downcast_ref::<CellularContent>()
                .unwrap();

            assert!(concrete_child_content.has_incoming_channels().await);
            assert_eq!(concrete_child_content.count_incoming_channels().await, 6);

            Ok(())
        }

        #[tokio::test]
        async fn test_linking_two_nodes_by_try_link_child() {
            let node_id = 0;
            let content = CellularContent::new(3, node_id);
            let root_node = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            let node_id = 1;
            let content = CellularContent::new(2, node_id);
            let child_node = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            let op_res = root_node.try_link_child(child_node.clone());
            assert!(op_res.is_ok());
            assert!(op_res.unwrap());

            assert!(!root_node.has_parents().await);
            assert!(root_node.has_children().await);
            assert!(child_node.has_parents().await);
            assert!(!child_node.has_children().await);

            let dyn_root_content = root_node.content.read().await;
            let concrete_root_content = dyn_root_content
                .as_any()
                .downcast_ref::<CellularContent>()
                .unwrap();

            assert!(!concrete_root_content.has_incoming_channels().await);
            assert_eq!(concrete_root_content.count_incoming_channels().await, 0);

            let dyn_child_content = child_node.content.read().await;
            let concrete_child_content = dyn_child_content
                .as_any()
                .downcast_ref::<CellularContent>()
                .unwrap();

            assert!(concrete_child_content.has_incoming_channels().await);
            assert_eq!(concrete_child_content.count_incoming_channels().await, 6);
        }

        #[tokio::test]
        async fn test_second_link_child_attempt_should_return_false() -> Result<(), Box<dyn Error>>
        {
            let node_id = 0;
            let content = CellularContent::new(3, node_id);
            let parent = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            let node_id = 1;
            let content = CellularContent::new(2, node_id);
            let child = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            assert!(parent.link_child(child.clone()).await?);
            assert!(!parent.link_child(child.clone()).await?);

            let dyn_child_content = child.content.read().await;
            let concrete_child_content = dyn_child_content
                .as_any()
                .downcast_ref::<CellularContent>()
                .unwrap();

            assert!(concrete_child_content.has_incoming_channels().await);
            assert_eq!(concrete_child_content.count_incoming_channels().await, 6);

            Ok(())
        }

        #[tokio::test]
        async fn test_second_try_link_child_attempt_should_return_false()
        -> Result<(), Box<dyn Error>> {
            let node_id = 0;
            let content = CellularContent::new(3, node_id);
            let parent = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            let node_id = 1;
            let content = CellularContent::new(2, node_id);
            let child = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            assert!(parent.try_link_child(child.clone())?);
            assert!(!parent.try_link_child(child.clone())?);

            let dyn_child_content = child.content.read().await;
            let concrete_child_content = dyn_child_content
                .as_any()
                .downcast_ref::<CellularContent>()
                .unwrap();

            assert!(concrete_child_content.has_incoming_channels().await);
            assert_eq!(concrete_child_content.count_incoming_channels().await, 6);

            Ok(())
        }

        #[tokio::test]
        async fn test_second_link_parent_attempt_should_return_false() -> Result<(), Box<dyn Error>>
        {
            let node_id = 0;
            let content = CellularContent::new(3, node_id);
            let parent = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            let node_id = 1;
            let content = CellularContent::new(2, node_id);
            let child = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            assert!(child.link_parent(parent.clone()).await?);
            assert!(!child.link_parent(parent.clone()).await?);

            let dyn_child_content = child.content.read().await;
            let concrete_child_content = dyn_child_content
                .as_any()
                .downcast_ref::<CellularContent>()
                .unwrap();

            assert!(concrete_child_content.has_incoming_channels().await);
            assert_eq!(concrete_child_content.count_incoming_channels().await, 6);

            Ok(())
        }

        #[tokio::test]
        async fn test_unlink_child_should_break_link_between_linked_nodes()
        -> Result<(), Box<dyn Error>> {
            let node_id = 0;
            let content = CellularContent::new(3, node_id);
            let parent = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            let node_id = 1;
            let content = CellularContent::new(2, node_id);
            let child = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            assert!(parent.link_child(child.clone()).await?);

            let dyn_child_content = child.content.read().await;
            let concrete_child_content = dyn_child_content
                .as_any()
                .downcast_ref::<CellularContent>()
                .unwrap();

            assert!(concrete_child_content.has_incoming_channels().await);
            assert_eq!(concrete_child_content.count_incoming_channels().await, 6);

            assert!(parent.unlink_child(child.clone()).await?);

            assert!(!parent.has_children().await);
            assert!(!child.has_parents().await);

            let dyn_child_content = child.content.read().await;
            let concrete_child_content = dyn_child_content
                .as_any()
                .downcast_ref::<CellularContent>()
                .unwrap();

            assert!(!concrete_child_content.has_incoming_channels().await);
            assert_eq!(concrete_child_content.count_incoming_channels().await, 0);

            Ok(())
        }

        #[tokio::test]
        async fn test_second_unlink_child_should_return_false() -> Result<(), Box<dyn Error>> {
            let node_id = 0;
            let content = CellularContent::new(3, node_id);
            let parent = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            let node_id = 1;
            let content = CellularContent::new(2, node_id);
            let child = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            assert!(parent.link_child(child.clone()).await?);

            assert!(parent.unlink_child(child.clone()).await?);
            assert!(!parent.unlink_child(child.clone()).await?);

            let dyn_child_content = child.content.read().await;
            let concrete_child_content = dyn_child_content
                .as_any()
                .downcast_ref::<CellularContent>()
                .unwrap();

            assert!(!concrete_child_content.has_incoming_channels().await);
            assert_eq!(concrete_child_content.count_incoming_channels().await, 0);

            Ok(())
        }

        #[tokio::test]
        async fn test_unlink_parent_should_break_link_between_linked_nodes()
        -> Result<(), Box<dyn Error>> {
            let node_id = 0;
            let content = CellularContent::new(3, node_id);
            let parent = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            let node_id = 1;
            let content = CellularContent::new(2, node_id);
            let child = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            assert!(parent.link_child(child.clone()).await?);

            assert!(child.unlink_parent(parent.clone()).await?);

            assert!(!parent.has_children().await);
            assert!(!child.has_parents().await);

            let dyn_child_content = child.content.read().await;
            let concrete_child_content = dyn_child_content
                .as_any()
                .downcast_ref::<CellularContent>()
                .unwrap();

            assert!(!concrete_child_content.has_incoming_channels().await);
            assert_eq!(concrete_child_content.count_incoming_channels().await, 0);

            Ok(())
        }

        #[tokio::test]
        async fn test_second_unlink_parent_should_return_false() -> Result<(), Box<dyn Error>> {
            let node_id = 0;
            let content = CellularContent::new(3, node_id);
            let parent = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            let node_id = 1;
            let content = CellularContent::new(2, node_id);
            let child = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            assert!(child.link_parent(parent.clone()).await?);

            assert!(child.unlink_parent(parent.clone()).await?);
            assert!(!child.unlink_parent(parent.clone()).await?);

            let dyn_child_content = child.content.read().await;
            let concrete_child_content = dyn_child_content
                .as_any()
                .downcast_ref::<CellularContent>()
                .unwrap();

            assert!(!concrete_child_content.has_incoming_channels().await);
            assert_eq!(concrete_child_content.count_incoming_channels().await, 0);

            Ok(())
        }

        #[tokio::test]
        async fn test_child_ids_should_return_children_identifiers_in_vector()
        -> Result<(), Box<dyn Error>> {
            let node_id = 0;
            let content = CellularContent::new(3, node_id);
            let parent = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            let node_id = 1;
            let content = CellularContent::new(2, node_id);
            let child1 = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            let node_id = 2;
            let content = CellularContent::new(4, node_id);
            let child2 = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            assert!(parent.link_child(child1.clone()).await?);
            assert!(parent.link_child(child2.clone()).await?);

            let ids = parent.child_ids().await;

            assert!(!ids.is_empty());
            assert_eq!(ids.len(), 2);
            assert!(ids.contains(&1));
            assert!(ids.contains(&2));

            let dyn_parent_content = parent.content.read().await;
            let concrete_parent_content = dyn_parent_content
                .as_any()
                .downcast_ref::<CellularContent>()
                .unwrap();

            assert_eq!(concrete_parent_content.count_outgoing_channels().await, 18);

            Ok(())
        }

        #[tokio::test]
        async fn test_children_ids_should_return_children_identifiers_in_vector()
        -> Result<(), Box<dyn Error>> {
            let node_id = 0;
            let content = CellularContent::new(3, node_id);
            let parent1 = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            let node_id = 1;
            let content = CellularContent::new(2, node_id);
            let parent2 = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            let node_id = 2;
            let content = CellularContent::new(4, node_id);
            let child = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            assert!(parent1.link_child(child.clone()).await?);
            assert!(parent2.link_child(child.clone()).await?);

            let ids = child.parent_ids().await;

            assert!(!ids.is_empty());
            assert_eq!(ids.len(), 2);
            assert!(ids.contains(&0));
            assert!(ids.contains(&1));

            let dyn_children_content = child.content.read().await;
            let concrete_children_content = dyn_children_content
                .as_any()
                .downcast_ref::<CellularContent>()
                .unwrap();

            assert_eq!(
                concrete_children_content.count_incoming_channels().await,
                20
            );

            Ok(())
        }

        #[tokio::test]
        async fn test_linking_two_nodes_by_link_parent() -> Result<(), Box<dyn Error>> {
            let node_id = 0;
            let content = CellularContent::new(3, node_id);
            let parent = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            let node_id = 1;
            let content = CellularContent::new(2, node_id);
            let child = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            assert!(child.link_parent(parent.clone()).await?);

            assert!(!parent.has_parents().await);
            assert!(parent.has_children().await);
            assert!(child.has_parents().await);
            assert!(!child.has_children().await);

            let dyn_children_content = child.content.read().await;
            let concrete_children_content = dyn_children_content
                .as_any()
                .downcast_ref::<CellularContent>()
                .unwrap();

            assert_eq!(concrete_children_content.count_incoming_channels().await, 6);

            Ok(())
        }

        #[tokio::test]
        async fn test_has_child_should_return_true_with_correct_child_id()
        -> Result<(), Box<dyn Error>> {
            let node_id = 0;
            let content = CellularContent::new(3, node_id);
            let parent = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            let node_id = 1;
            let content = CellularContent::new(2, node_id);
            let child = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            assert!(parent.link_child(child.clone()).await?);

            assert!(parent.has_child(&1).await);

            Ok(())
        }

        #[tokio::test]
        async fn test_has_child_should_return_false_with_incorrect_child_id() {
            let node_id = 0;
            let content = CellularContent::new(3, node_id);
            let node = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            assert!(!node.has_child(&1).await);
        }

        #[tokio::test]
        async fn test_has_parent_should_return_true_with_correct_parent_id()
        -> Result<(), Box<dyn Error>> {
            let node_id = 0;
            let content = CellularContent::new(3, node_id);
            let parent = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            let node_id = 1;
            let content = CellularContent::new(2, node_id);
            let child = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            assert!(child.link_parent(parent.clone()).await?);

            assert!(child.has_parent(&0).await);

            Ok(())
        }

        #[tokio::test]
        async fn test_has_parent_should_return_false_with_incorrect_parent_id() {
            let node_id = 0;
            let content = CellularContent::new(3, node_id);
            let node = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            assert!(!node.has_parent(&1).await);
        }

        #[tokio::test]
        async fn test_not_allow_link_self_node_as_child() {
            let node_id = 0;
            let content = CellularContent::new(3, node_id);
            let node = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            assert!(node.link_child(node.clone()).await.is_err());
        }

        #[tokio::test]
        async fn test_allow_cyclic_links_between_two_nodes() -> Result<(), Box<dyn Error>> {
            let node_id = 0;
            let content = CellularContent::new(3, node_id);
            let node0 = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            let node_id = 1;
            let content = CellularContent::new(2, node_id);
            let node1 = Arc::new(Node::new(node_id, Arc::new(RwLock::new(content))));

            assert!(node0.link_child(node1.clone()).await?);
            assert!(node1.link_child(node0.clone()).await?);

            assert!(node0.has_child(node1.id()).await);
            assert!(node1.has_child(node0.id()).await);

            assert!(node0.has_parent(node1.id()).await);
            assert!(node1.has_parent(node0.id()).await);

            let dyn_node0_content = node0.content.read().await;
            let concrete_node0_content = dyn_node0_content
                .as_any()
                .downcast_ref::<CellularContent>()
                .unwrap();

            assert!(concrete_node0_content.has_incoming_channels().await);
            assert_eq!(concrete_node0_content.count_incoming_channels().await, 6);

            let dyn_node1_content = node1.content.read().await;
            let concrete_node1_content = dyn_node1_content
                .as_any()
                .downcast_ref::<CellularContent>()
                .unwrap();

            assert!(concrete_node1_content.has_incoming_channels().await);
            assert_eq!(concrete_node1_content.count_incoming_channels().await, 6);

            Ok(())
        }
    }
}

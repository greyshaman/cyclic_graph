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
    ///     async fn data(&self) -> Arc<RwLock<String>> {
    ///         self.data.clone()
    ///     }
    ///
    ///     async fn set_data(
    ///         &mut self,
    ///         data: Arc<RwLock<String>>
    ///     ) -> Result<Arc<RwLock<String>>, CGError<usize>> {
    ///         let prev = self.data.clone();
    ///         self.data = data.clone();
    ///         Ok(prev)
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
    ///     async fn data(&self) -> Arc<RwLock<Vec<usize>>> {
    ///         self.data.clone()
    ///     }
    ///
    ///     async fn set_data(
    ///         &mut self,
    ///         data: Arc<RwLock<Vec<usize>>>,
    ///     ) -> Result<Arc<RwLock<Vec<usize>>>, CGError<String>> {
    ///         let prev = self.data.clone();
    ///         self.data = data.clone();
    ///         Ok(prev)
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
        self.content.read().await.data().await
    }

    /// Changes wrapped payload data
    pub async fn set_data(&self, value: Arc<RwLock<D>>) -> Result<Arc<RwLock<D>>, CGError<I>> {
        self.content.write().await.set_data(value).await
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
        let res = self.child_ids.write().await.insert(child.id().clone())
            && child.parent_ids.write().await.insert(self.id().clone());
        if res {
            return child
                .content
                .read()
                .await
                .link_accept(self.content.clone())
                .await;
        }

        Ok(res)
    }

    /// Creates link to specified child and from child to current node as to parent synchronously
    pub fn try_link_child(&self, child: Arc<Node<I, D, S>>) -> Result<bool, CGError<I>> {
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
        let res = self.parent_ids.write().await.insert(parent.id().clone())
            && parent.child_ids.write().await.insert(self.id().clone());
        if res {
            return self
                .content
                .read()
                .await
                .link_accept(parent.content.clone())
                .await;
        }

        Ok(res)
    }

    /// Creates link to specified parent and from parent to current node as to child synchronously
    pub fn try_link_parent(&self, parent: Arc<Node<I, D, S>>) -> Result<bool, CGError<I>> {
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
        use std::error::Error;

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
            async fn data(&self) -> Arc<RwLock<String>> {
                self.data.clone()
            }

            async fn set_data(
                &mut self,
                data: Arc<RwLock<String>>,
            ) -> Result<Arc<RwLock<String>>, CGError<usize>> {
                let ret = self.data.clone();
                self.data = data.clone();
                Ok(ret)
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
            async fn data(&self) -> Arc<RwLock<Vec<usize>>> {
                self.data.clone()
            }

            async fn set_data(
                &mut self,
                data: Arc<RwLock<Vec<usize>>>,
            ) -> Result<Arc<RwLock<Vec<usize>>>, CGError<usize>> {
                let prev = self.data.clone();
                self.data = data.clone();
                Ok(prev)
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

        #[tokio::test]
        async fn test_second_try_link_child_attempt_should_return_false() {
            let parent = Arc::new(Node::new(
                0_usize,
                Arc::new(RwLock::new(StringContent::new("parent"))),
            ));
            let child = Arc::new(Node::new(
                1_usize,
                Arc::new(RwLock::new(StringContent::new("child"))),
            ));

            assert!(parent.try_link_child(child.clone()).unwrap());
            assert!(!parent.try_link_child(child.clone()).unwrap());
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
        async fn test_allow_link_self_node_as_child() -> Result<(), Box<dyn Error>> {
            let node = Arc::new(Node::new(
                0_usize,
                Arc::new(RwLock::new(StringContent::new("test"))),
            ));

            assert!(node.link_child(node.clone()).await?);

            assert!(node.has_child(node.id()).await);

            Ok(())
        }
    }

    mod for_id_as_str {
        use super::*;
        use async_trait::async_trait;
        use std::error::Error;

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
            async fn data(&self) -> Arc<RwLock<String>> {
                self.data.clone()
            }

            async fn set_data(
                &mut self,
                data: Arc<RwLock<String>>,
            ) -> Result<Arc<RwLock<String>>, CGError<&'static str>> {
                let ret = self.data.clone();
                self.data = data.clone();
                Ok(ret)
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
            async fn data(&self) -> Arc<RwLock<Vec<usize>>> {
                self.data.clone()
            }

            async fn set_data(
                &mut self,
                data: Arc<RwLock<Vec<usize>>>,
            ) -> Result<Arc<RwLock<Vec<usize>>>, CGError<&'static str>> {
                let prev = self.data.clone();
                self.data = data.clone();
                Ok(prev)
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
        async fn test_allow_link_self_node_as_child() -> Result<(), Box<dyn Error>> {
            let node = Arc::new(Node::new(
                "n0",
                Arc::new(RwLock::new(StringContent::new("test"))),
            ));

            assert!(node.link_child(node.clone()).await?);

            assert!(node.has_child(node.id()).await);

            Ok(())
        }
    }
}

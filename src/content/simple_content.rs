use std::{fmt::Debug, sync::Arc};

use tokio::sync::RwLock;

/// A simple content implementation. Repsents a boxed value that can be read and written.
#[derive(Debug, Clone)]
pub struct SimpleContent<D>
where
    D: Clone + Debug,
{
    value: Arc<RwLock<D>>,
}

impl<D> SimpleContent<D>
where
    D: Clone + Debug,
{
    /// Creates a new simple content.
    pub fn new(value: D) -> Self {
        Self {
            value: Arc::new(RwLock::new(value)),
        }
    }

    /// Gets the content data.
    pub fn content(&self) -> Arc<RwLock<D>> {
        self.value.clone()
    }

    /// Gets the cloned value.
    pub async fn value(&self) -> D {
        let value_binding = self.value.read().await;
        value_binding.clone()
    }

    /// Sets the new value. Returns the old value.
    pub async fn set_value(&self, value: D) -> Option<D> {
        let mut value_binding = self.value.write().await;
        let old_value = value_binding.clone();
        *value_binding = value;
        Some(old_value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_i32_simple_content() {
        let value = 1;
        let content = SimpleContent::new(value);
        assert_eq!(content.value().await, value);
    }

    #[tokio::test]
    async fn test_create_string_simple_content() {
        let value = "one";
        let content = SimpleContent::new(value.to_string());
        assert_eq!(content.value().await, value);
    }

    #[tokio::test]
    async fn test_create_vector_simple_content() {
        let value = vec![1_i32, 2, 3];
        let content = SimpleContent::new(value.clone());
        assert_eq!(&content.value().await.len(), &value.len());
        assert_eq!(&content.value().await[0], &value[0]);
        assert_eq!(&content.value().await[1], &value[1]);
        assert_eq!(&content.value().await[2], &value[2]);
        assert!(!content.value().await.contains(&4));
    }

    #[tokio::test]
    async fn test_set_i32_simple_content() {
        let content = SimpleContent::<i32>::new(1);
        let new_value = 2;
        assert_eq!(content.set_value(new_value).await.unwrap(), 1);
        assert_eq!(content.value().await, new_value);
    }

    #[tokio::test]
    async fn test_set_string_simple_content() {
        let content = SimpleContent::<String>::new("one".to_string());
        let new_value = "two".to_string();
        assert_eq!(content.set_value(new_value.clone()).await.unwrap(), "one");
        assert_eq!(content.value().await, new_value);
    }

    #[tokio::test]
    async fn test_set_vector_simple_content() {
        let content = SimpleContent::<Vec<i32>>::new(vec![1, 2, 3]);
        let new_value = vec![4, 5, 6];
        assert_eq!(
            content.set_value(new_value.clone()).await.unwrap(),
            vec![1, 2, 3]
        );
        assert_eq!(&content.value().await.len(), &new_value.len());
        assert_eq!(&content.value().await[0], &new_value[0]);
        assert_eq!(&content.value().await[1], &new_value[1]);
        assert_eq!(&content.value().await[2], &new_value[2]);
    }
}

use std::{fmt::Debug, sync::Arc};

use crate::Error as CGError;
use async_trait::async_trait;
use tokio::sync::{RwLock, broadcast};

#[async_trait]
pub trait Content<I: 'static, D: 'static, S: 'static>: Debug + Send + Sync {
    async fn data(&self) -> Arc<RwLock<D>>;

    async fn set_data(&mut self, data: Arc<RwLock<D>>) -> Result<Arc<RwLock<D>>, CGError<I>>;

    async fn provide_receiver(
        &mut self,
        _src_idx: usize,
    ) -> Result<Option<broadcast::Receiver<S>>, CGError<I>> {
        Ok(None)
    }

    fn try_provide_receiver(
        &mut self,
        _src_idx: usize,
    ) -> Result<Option<broadcast::Receiver<S>>, CGError<I>> {
        Ok(None)
    }

    async fn link_accept(
        &self,
        _provider: Arc<RwLock<dyn Content<I, D, S> + Send + Sync>>,
    ) -> Result<bool, CGError<I>> {
        Ok(true)
    }

    fn try_link_accept(
        &self,
        _provider: Arc<RwLock<dyn Content<I, D, S> + Send + Sync>>,
    ) -> Result<bool, CGError<I>> {
        Ok(true)
    }

    async fn link_disconnect(
        &self,
        _provider: Arc<RwLock<dyn Content<I, D, S> + Send + Sync>>,
    ) -> Result<bool, CGError<I>> {
        Ok(true)
    }

    fn try_link_disconnect(
        &self,
        _provider: Arc<RwLock<dyn Content<I, D, S> + Send + Sync>>,
    ) -> Result<bool, CGError<I>> {
        Ok(true)
    }
}

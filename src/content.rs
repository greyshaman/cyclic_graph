use std::{any::Any, fmt::Debug, sync::Arc};

use crate::Error as CGError;
use async_trait::async_trait;
use tokio::sync::{RwLock, broadcast};

#[async_trait]
pub trait Content: Debug + Send + Sync + Any {
    type IdType: 'static + Send + Sync;
    type PayloadType: 'static + Send + Sync + Any;
    type SignalType: 'static + Send + Sync;

    fn as_any(&self) -> &dyn Any;

    fn data(&self) -> Arc<RwLock<Self::PayloadType>>;

    async fn data_value(&self) -> Option<Self::PayloadType> {
        None
    }

    async fn set_data_value(
        &self,
        _value: Self::PayloadType,
    ) -> Result<Option<Self::PayloadType>, CGError<Self::IdType>> {
        Ok(None)
    }

    async fn provide_receiver(
        &self,
        _src_idx: Self::IdType,
    ) -> Result<Option<broadcast::Receiver<Self::SignalType>>, CGError<Self::IdType>> {
        Ok(None)
    }

    fn try_provide_receiver(
        &self,
        _src_idx: Self::IdType,
    ) -> Result<Option<broadcast::Receiver<Self::SignalType>>, CGError<Self::IdType>> {
        Ok(None)
    }

    async fn provide_src_ids(&self) -> Vec<Self::IdType> {
        vec![]
    }

    fn try_provide_src_ids(&self) -> Result<Vec<Self::IdType>, CGError<Self::IdType>> {
        Ok(vec![])
    }

    async fn link_accept(
        &self,
        _provider: Arc<
            RwLock<
                dyn Content<
                        IdType = Self::IdType,
                        PayloadType = Self::PayloadType,
                        SignalType = Self::SignalType,
                    > + Send
                    + Sync,
            >,
        >,
    ) -> Result<bool, CGError<Self::IdType>> {
        Ok(true)
    }

    fn try_link_accept(
        &self,
        _provider: Arc<
            RwLock<
                dyn Content<
                        IdType = Self::IdType,
                        PayloadType = Self::PayloadType,
                        SignalType = Self::SignalType,
                    > + Send
                    + Sync,
            >,
        >,
    ) -> Result<bool, CGError<Self::IdType>> {
        Ok(true)
    }

    async fn link_disconnect(
        &self,
        _provider: Arc<
            RwLock<
                dyn Content<
                        IdType = Self::IdType,
                        PayloadType = Self::PayloadType,
                        SignalType = Self::SignalType,
                    > + Send
                    + Sync,
            >,
        >,
    ) -> Result<bool, CGError<Self::IdType>> {
        Ok(true)
    }

    fn try_link_disconnect(
        &self,
        _provider: Arc<
            RwLock<
                dyn Content<
                        IdType = Self::IdType,
                        PayloadType = Self::PayloadType,
                        SignalType = Self::SignalType,
                    > + Send
                    + Sync,
            >,
        >,
    ) -> Result<bool, CGError<Self::IdType>> {
        Ok(true)
    }
}

use thiserror::Error;

#[derive(Error, Debug)]
pub enum CyclicGraphError {
    #[error("Cannot insert node before input node")]
    InsertBeforeInput,

    #[error("Cannot insert node after output node")]
    InsertAfterOutput,

    #[error("Node not found by id: {0}")]
    NodeNotFoundById(usize),
}

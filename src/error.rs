use thiserror::Error;

#[derive(Error, Debug)]
pub enum CyclicGraphError<I> {
    #[error("Cannot insert node before input node")]
    InsertBeforeInput,

    #[error("Cannot insert node after output node")]
    InsertAfterOutput,

    #[error("Node not found by id: {0}")]
    NodeNotFoundById(I),

    #[error("Entered id `{0}` is not unique")]
    NonUniqueId(I),
}

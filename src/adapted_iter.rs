use tokio_stream::Stream;

use crate::adapters::AdaptInfo;

pub trait AdaptedFilesIter: Stream<Item = AdaptInfo> + Send + Unpin {}
impl<T> AdaptedFilesIter for T where T: Stream<Item = AdaptInfo> + Send + Unpin {}

pub type AdaptedFilesIterBox = Box<dyn AdaptedFilesIter>;

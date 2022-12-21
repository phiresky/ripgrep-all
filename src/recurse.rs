use tokio_util::io::{ReaderStream, StreamReader};

use crate::{adapted_iter::AdaptedFilesIterBox, adapters::*};
use async_stream::stream;

pub fn concat_read_streams(input: AdaptedFilesIterBox) -> ReadBox {
    let s = stream! {
        for await output in input {
            let stream = ReaderStream::new(output.inp);
            for await bytes in stream {
                yield bytes;
            }
        }
    };
    Box::pin(StreamReader::new(s))
}

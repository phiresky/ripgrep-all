use tokio_util::io::{ReaderStream, StreamReader};

use crate::{adapted_iter::AdaptedFilesIterBox, adapters::*, to_io_err};
use async_stream::stream;

pub fn concat_read_streams(input: AdaptedFilesIterBox) -> ReadBox {
    let s = stream! {
        for await output in input {
            let o = output.map_err(to_io_err)?.inp;
            let stream = ReaderStream::new(o);
            for await bytes in stream {
                yield bytes;
            }
        }
    };
    Box::pin(StreamReader::new(s))
}

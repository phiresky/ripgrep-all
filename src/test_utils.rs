use crate::{
    adapted_iter::AdaptedFilesIterBox,
    adapters::{AdaptInfo, ReadBox},
    config::RgaConfig,
    matching::{FastFileMatcher, FileMatcher},
    recurse::RecursingConcattyReader,
};
use anyhow::Result;
use std::path::{Path, PathBuf};

pub fn test_data_dir() -> PathBuf {
    let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    d.push("exampledir/test/");
    d
}

pub fn simple_adapt_info<'a>(filepath: &Path, inp: ReadBox<'a>) -> (AdaptInfo<'a>, FileMatcher) {
    (
        AdaptInfo {
            filepath_hint: filepath.to_owned(),
            is_real_file: true,
            archive_recursion_depth: 0,
            inp,
            line_prefix: "PREFIX:".to_string(),
            config: RgaConfig::default(),
            postprocess: true,
        },
        FastFileMatcher::FileExtension(
            filepath.extension().unwrap().to_string_lossy().into_owned(),
        )
        .into(),
    )
}

pub fn adapted_to_vec(adapted: AdaptedFilesIterBox<'_>) -> Result<Vec<u8>> {
    let mut res = RecursingConcattyReader::concat(adapted)?;

    let mut buf = Vec::new();
    res.read_to_end(&mut buf)?;
    Ok(buf)
}

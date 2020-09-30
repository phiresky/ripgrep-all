use crate::{
    adapters::{AdaptInfo, ReadBox},
    config::RgaConfig,
    matching::{FastFileMatcher, FileMatcher},
};
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
        },
        FastFileMatcher::FileExtension(
            filepath.extension().unwrap().to_string_lossy().into_owned(),
        )
        .into(),
    )
}

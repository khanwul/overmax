//! V-Archive API query facade.
//!
//! Re-exports from `crate::gateway::varchive` for backward compatibility.

pub use crate::gateway::varchive::{
    fetch_records_blocking, fetch_single_song_records_blocking, parse_account_file,
    upload_score_blocking, AccountInfo, UploadResult,
};

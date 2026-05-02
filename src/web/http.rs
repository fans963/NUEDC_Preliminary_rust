use crate::display::pool;
use crate::fsm::PovState;

/// Static file and JSON response service for the POV dashboard.
pub struct WebService;

impl WebService {
    pub fn get_index_html() -> &'static str {
        include_str!("../../frontend/index.html")
    }

    /// Generate JSON status response.
    pub fn json_status(state: &PovState) -> alloc::string::String {
        use alloc::format;
        let name = state.state_name();
        let count = pool::animation_count();
        let pages = pool::page_count();
        format!(
            r#"{{"state":"{}","is_running":{},"is_loading":{},"animation_count":{},"pages":{}}}"#,
            name,
            name == "running",
            name == "loading",
            count,
            pages,
        )
    }
}


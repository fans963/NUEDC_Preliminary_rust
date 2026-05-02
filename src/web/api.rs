use alloc::format;
use pov_core::serial::Animation;
use crate::display::pool;
use crate::fsm::{Event, PovState};
use crate::web::http::WebService;

pub struct ApiRouter;

#[derive(Debug)]
pub enum ApiRoute {
    Status,
    Start,
    Stop,
    Upload,
    NotFound,
}

impl ApiRouter {
    pub fn handle_get(route: &ApiRoute, state: &PovState) -> alloc::string::String {
        match route {
            ApiRoute::Status => WebService::json_status(state),
            ApiRoute::NotFound => format!(r#"{{"error":"not found"}}"#),
            _ => format!(r#"{{"error":"method not allowed"}}"#),
        }
    }

    pub fn handle_post(
        route: &ApiRoute,
        state: &PovState,
        body: Option<&[u8]>,
    ) -> alloc::string::String {
        match route {
            ApiRoute::Start => {
                state.handle(&Event::Start);
                format!(r#"{{"status":"started"}}"#)
            }
            ApiRoute::Stop => {
                state.handle(&Event::Stop);
                format!(r#"{{"status":"stopped"}}"#)
            }
            ApiRoute::Upload => {
                match body {
                    Some(data) => {
                        match postcard::from_bytes::<Animation>(data) {
                            Ok(anim) => {
                                match pool::set_animation(anim) {
                                    Ok(()) => {
                                        format!(
                                            r#"{{"status":"ok","pages":{}}}"#,
                                            pool::page_count(),
                                        )
                                    }
                                    Err(e) => format!(r#"{{"status":"error","message":"{}"}}"#, e),
                                }
                            }
                            Err(e) => format!(r#"{{"status":"error","message":"{:?}"}}"#, e),
                        }
                    }
                    None => format!(r#"{{"status":"error","message":"no body"}}"#),
                }
            }
            _ => format!(r#"{{"error":"method not allowed"}}"#),
        }
    }
}

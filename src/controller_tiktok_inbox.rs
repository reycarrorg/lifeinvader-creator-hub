use axum::{http::StatusCode,Json};
use serde_json::{json,Value};
pub(crate) fn status()->Value{json!({"provider":"tiktok","health":"manual","mode":"local_export_only","public_posting":false,"inbox_enabled":false})}
pub(crate) async fn draft()->(StatusCode,Json<Value>){(StatusCode::FORBIDDEN,Json(json!({"reason":"tiktok_manual_export_only","public_posting":false})))}

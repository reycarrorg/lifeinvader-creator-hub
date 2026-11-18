use super::*;
use std::collections::HashSet;

#[derive(Serialize)]
pub struct Category {
    role: String,
    name: String,
}
#[derive(Deserialize)]
pub struct Input {
    role: String,
    name: String,
}
#[derive(Deserialize)]
pub struct Membership {
    role: String,
    name: String,
    profile_ids: Vec<i64>,
}

fn validate(role: &str, name: &str) -> ApiResult<String> {
    let name = name.trim();
    if !["source", "destination"].contains(&role)
        || name.is_empty()
        || name.len() > 50
        || name.contains(',')
        || name.chars().any(char::is_control)
    {
        return Err(bad(
            "Enter a category name up to 50 characters without commas.",
        ));
    }
    Ok(name.to_owned())
}
pub fn list(db: &Connection) -> rusqlite::Result<Vec<Category>> {
    let mut stmt = db.prepare("SELECT role,name FROM categories ORDER BY name COLLATE NOCASE")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Category {
                role: r.get(0)?,
                name: r.get(1)?,
            })
        })?
        .collect();
    rows
}
pub async fn create(State(app): State<App>, Json(input): Json<Input>) -> ApiResult<StatusCode> {
    let name = validate(&input.role, &input.name)?;
    let db = app.db.lock().map_err(internal)?;
    db.execute(
        "INSERT OR IGNORE INTO categories(role,name) VALUES(?,?)",
        params![input.role, name],
    )
    .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}
pub async fn set_members(
    State(app): State<App>,
    Json(input): Json<Membership>,
) -> ApiResult<StatusCode> {
    let name = validate(&input.role, &input.name)?;
    let selected: HashSet<i64> = input.profile_ids.iter().copied().collect();
    if selected.len() > 1000 {
        return Err(bad("Too many profiles."));
    }
    let mut db = app.db.lock().map_err(internal)?;
    let tx = db.transaction().map_err(internal)?;
    let rows: Vec<(i64, String)> = {
        let mut stmt = tx
            .prepare("SELECT id,tags FROM profiles WHERE role=?")
            .map_err(internal)?;
        let rows = stmt
            .query_map([&input.role], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(internal)?
            .collect::<rusqlite::Result<_>>()
            .map_err(internal)?;
        rows
    };
    if selected.iter().any(|id| !rows.iter().any(|r| r.0 == *id)) {
        return Err(bad("A selected account is not in this section."));
    }
    for (id, raw) in rows {
        let mut tags: Vec<String> = serde_json::from_str(&raw).map_err(internal)?;
        let has = tags.iter().any(|t| t.eq_ignore_ascii_case(&name));
        if selected.contains(&id) && !has {
            if tags.len() >= 20 {
                return Err(bad("An account already has 20 categories."));
            }
            tags.push(name.clone());
        }
        if !selected.contains(&id) && has {
            tags.retain(|t| !t.eq_ignore_ascii_case(&name));
        }
        tx.execute(
            "UPDATE profiles SET tags=? WHERE id=?",
            params![serde_json::to_string(&tags).map_err(internal)?, id],
        )
        .map_err(internal)?;
    }
    tx.execute(
        "INSERT OR IGNORE INTO categories(role,name) VALUES(?,?)",
        params![input.role, name],
    )
    .map_err(internal)?;
    tx.commit().map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

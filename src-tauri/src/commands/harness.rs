//! Harness commands (M16).

use tauri::State;

use super::blocking;
use crate::db::{missions, now_ms, Db};
use crate::error::AppResult;
use crate::harness::{self, HarnessInput, Named, Preference, ProfileInfo, ProviderInfo, Recommendation, UsageStat};

const WINDOW_MS: i64 = 30 * 24 * 3600 * 1000;

#[tauri::command]
pub async fn harness_recommend(db: State<'_, Db>, text: String, preference: Option<Preference>) -> AppResult<Recommendation> {
    let (checks, profiles, skills, servers, usage) = db.with(|c| {
        let checks: Vec<_> = crate::providers::PROVIDERS.iter().map(|p| crate::providers::load_check(c, p.id)).collect::<AppResult<_>>()?;
        let profiles: Vec<ProfileInfo> = crate::agents::profiles::list(c)?
            .into_iter()
            .map(|p| ProfileInfo { id: p.id, name: p.name, role: p.role, provider_id: p.provider_id, model: p.model, permission: p.permission, max_turns: p.max_turns })
            .collect();
        let skills: Vec<Named> = crate::skills::repo::list(c)?
            .into_iter()
            .filter(|s| s.enabled)
            .map(|s| Named { haystack: format!("{} {} {}", s.name, s.slug, s.description), id: s.id, name: s.name })
            .collect();
        let servers: Vec<Named> = crate::mcp::repo::list(c)?
            .into_iter()
            .filter(|s| s.enabled)
            .map(|s| Named { haystack: format!("{} {} {}", s.name, s.label, s.template_id.clone().unwrap_or_default()), id: s.id, name: s.name })
            .collect();
        let usage = harness::usage(c, now_ms() - WINDOW_MS)?;
        Ok((checks, profiles, skills, servers, usage))
    })?;
    blocking(move || {
        let providers: Vec<ProviderInfo> = std::thread::scope(|s| {
            let hs: Vec<_> = crate::providers::PROVIDERS.iter().zip(checks).map(|(p, chk)| s.spawn(move || crate::providers::status(p, chk))).collect();
            hs.into_iter()
                .filter_map(|h| h.join().ok())
                .map(|st| ProviderInfo { id: st.id, name: st.name, installed: st.installed, logged_out: st.auth.state == "logged_out", verified: st.verified })
                .collect()
        });
        Ok(harness::recommend(&HarnessInput {
            text: &text,
            preference: preference.unwrap_or_default(),
            providers: &providers,
            profiles: &profiles,
            skills: &skills,
            mcp: &servers,
            usage: &usage,
        }))
    })
    .await
}

#[tauri::command]
pub fn harness_usage(db: State<'_, Db>) -> AppResult<Vec<UsageStat>> {
    db.with(|c| harness::usage(c, now_ms() - WINDOW_MS))
}

#[tauri::command]
pub fn mission_set_harness(db: State<'_, Db>, id: String, enabled: bool) -> AppResult<missions::Mission> {
    db.with(|c| missions::set_harness(c, &id, enabled))
}

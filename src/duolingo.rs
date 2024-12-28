use anyhow::{bail, Result};
use chrono::{Local, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use ureq::Agent;

use crate::{DailyXPProgress, Lesson};

#[derive(Debug, Deserialize)]
struct JWTClaims {
    sub: serde_json::Value, // "sub" may be int or string
}

pub struct Duolingo {
    agent: Agent,
    jwt: String,
    username: String,
    user_id: String,
}

impl Duolingo {
    pub fn new(jwt: &str) -> Result<Self> {
        use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};

        // For ignoring the signature (like Python code verify=False):
        let mut validation = Validation::new(Algorithm::HS256);
        validation.insecure_disable_signature_validation();

        let token_data = decode::<JWTClaims>(
            jwt,
            &DecodingKey::from_secret(b"ignore_signature"),
            &validation,
        )
        .map_err(|e| anyhow::anyhow!("Failed to decode JWT's sub: {e}"))?;
        let user_id = token_data.claims.sub.to_string();

        let cookie_value = format!("jwt_token={}", jwt);
        let cookie_url: url::Url = "https://www.duolingo.com".parse().unwrap();
        let cookie =
            cookie_store::Cookie::parse(cookie_value, &cookie_url).expect("Failed to parse cookie");

        let mut store = cookie_store::CookieStore::new(None);
        store.insert(cookie, &cookie_url);
        // Build blocking agent
        let agent = ureq::AgentBuilder::new().cookie_store(store).build();

        let mut this = Self {
            agent,
            jwt: jwt.to_string(),
            username: String::new(),
            user_id,
        };
        this.set_username()?;
        if !this.check_auth()? {
            bail!("JWT or username invalid (check_auth failed)");
        }
        Ok(this)
    }

    /// Re-init from scratch
    pub fn update_jwt(&mut self, new_jwt: &str) -> Result<()> {
        *self = Duolingo::new(new_jwt)?;
        Ok(())
    }

    fn set_username(&mut self) -> Result<()> {
        let url = format!(
            "https://www.duolingo.com/2017-06-30/users/{}?fields=username",
            self.user_id
        );
        let resp = self
            .agent
            .get(&url)
            .set("User-Agent", "DuoEnforcer/0.1")
            .set("Authorization", &format!("Bearer {}", self.jwt))
            .call()?;
        if resp.status() != 200 {
            bail!("Failed to fetch username (status={})", resp.status());
        }
        let val: serde_json::Value = resp.into_json()?;
        let username_val = val
            .get("username")
            .ok_or_else(|| anyhow::anyhow!("Missing 'username'"))?;
        let username_str = username_val
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Username not a string"))?;
        self.username = username_str.to_string();
        Ok(())
    }

    fn check_auth(&self) -> Result<bool> {
        let url = format!("https://duolingo.com/users/{}", self.username);
        let resp = self
            .agent
            .get(&url)
            .set("User-Agent", "DuoEnforcer/0.1")
            .set("Authorization", &format!("Bearer {}", self.jwt))
            .call()?;
        Ok(resp.status() == 200)
    }

    pub fn get_daily_xp_progress(&self) -> Result<DailyXPProgress> {
        let url = format!(
            "https://www.duolingo.com/2017-06-30/users/{}?fields=xpGoal,xpGains,streakData",
            self.user_id
        );
        let resp = self
            .agent
            .get(&url)
            .set("User-Agent", "DuoEnforcer/0.1")
            .set("Authorization", &format!("Bearer {}", self.jwt))
            .call()?;

        if resp.status() != 200 {
            bail!("daily xp fetch returned status={}", resp.status());
        }
        #[derive(Debug, Deserialize)]
        struct RespData {
            #[serde(rename = "xpGoal")]
            xp_goal: i64,
            #[serde(rename = "xpGains")]
            xp_gains: Vec<Lesson>,
            #[serde(rename = "streakData")]
            streak_data: StreakData,
        }
        #[derive(Debug, Deserialize)]
        struct StreakData {
            #[serde(rename = "updatedTimestamp")]
            updated_timestamp: i64,
        }

        let daily: RespData = resp.into_json()?;

        // The "reported_midnight"
        let reported_midnight =
            NaiveDateTime::from_timestamp_opt(daily.streak_data.updated_timestamp, 0)
                .unwrap_or_else(|| Utc::now().naive_utc());

        // local midnight
        let today = Local::now().date_naive();
        let midnight = today
            .and_hms_opt(0, 0, 0)
            .unwrap_or_else(|| Utc::now().naive_utc());
        let time_discrepancy = midnight - reported_midnight;
        let adjusted_midnight = if time_discrepancy < chrono::Duration::zero() {
            reported_midnight
        } else {
            reported_midnight + time_discrepancy
        };
        let cutoff_ts = adjusted_midnight.timestamp();

        let lessons_today: Vec<Lesson> = daily
            .xp_gains
            .into_iter()
            .filter(|l| l.time > cutoff_ts)
            .collect();
        let xp_today = lessons_today.iter().map(|l| l.xp).sum();

        Ok(DailyXPProgress {
            xp_goal: daily.xp_goal,
            lessons_today,
            xp_today,
        })
    }
}
